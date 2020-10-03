use crate::repository::{DBError, ErrorType, Repository, RoomData, TokenData};
use serde::export::Formatter;
use std::fmt;
use warp::{http::StatusCode, reply, Filter};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_BODY_SIZE: u64 = 1024 * 16;

const ENTRY_EXISTS_RESPONSE: &str = "Entry already exists";
const FORBIDDEN_ERROR_RESPONSE: &str = "Forbidden";
const INTERNAL_ERROR_RESPONSE: &str = "Internal error";
const WRONG_PARAMS_RESPONSE: &str = "Wrong params";
const KEYWORDS_PARAM: &str = "keywords";

pub struct HttpServer {
    repository: Box<dyn Repository>,
    params: Params,
}

pub struct Params {
    pub ip_address: [u8; 4],
    pub port: u16,
}

pub fn new(params: impl Into<Params>, repository: Box<dyn Repository>) -> HttpServer {
    HttpServer {
        params: params.into(),
        repository,
    }
}

#[derive(Deserialize)]
pub struct Login {
    room_name: String,
    password: Option<String>,
}

impl HttpServer {
    pub async fn run(self) {
        let repository_mtx = Arc::new(Mutex::new(self.repository));
        let repository_mtx = warp::any().map(move || repository_mtx.clone());

        let login = warp::post()
            .and(warp::path("login"))
            // Only accept bodies smaller than 16kb...
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(repository_mtx.clone())
            .and_then(login);

        let add_room = warp::post()
            .and(warp::path("rooms"))
            .and(warp::body::content_length_limit(MAX_BODY_SIZE))
            .and(warp::body::json())
            .and(repository_mtx.clone())
            .and_then(add_room);

        let list_rooms = warp::get()
            .and(warp::path("rooms"))
            .and(warp::query::<HashMap<String, String>>())
            .and(repository_mtx.clone())
            .and_then(list_rooms);
        let cors = warp::cors()
            .allow_any_origin()
            .allow_headers(vec![
                "User-Agent",
                "Sec-Fetch-Mode",
                "Referer",
                "Origin",
                "Access-Control-Request-Method",
                "Content-Type",
                "Access-Control-Request-Headers",
            ])
            .allow_methods(vec!["GET", "POST"]); // todo
        let routes = (login.or(add_room).or(list_rooms)).with(cors); // todo: remove cors

        warp::serve(routes)
            .run((self.params.ip_address, self.params.port))
            .await
    }
}

#[derive(Deserialize, Serialize)]
struct RoomsResp {
    data: Vec<RoomResp>,
}

#[derive(Deserialize, Serialize)]
struct RoomResp {
    pub name: String,
    pub password: bool,
    pub keywords: Option<Vec<String>>,
    pub description: Option<String>,
}

async fn list_rooms(
    mut query: HashMap<String, String>,
    repository: Arc<Mutex<Box<dyn Repository>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    debug!("list_rooms controller");

    let keywords = query.remove(KEYWORDS_PARAM);

    let keywords = match keywords {
        Some(k_str) => k_str,
        None => String::new(),
    };

    let keywords_param = keywords.split(",").collect();
    let repo = repository.lock().await;
    let room_r = repo.room();

    let res = room_r.find(keywords_param);

    return match res {
        Ok(rooms) => {
            let mut rooms_resp = Vec::new();

            for r in rooms {
                let password = match r.password {
                    Some(_) => true,
                    None => false,
                };
                let room_resp = RoomResp {
                    password,
                    keywords: r.keywords,
                    name: r.name,
                    description: r.description,
                };

                rooms_resp.push(room_resp);
            }

            let resp = RoomsResp { data: rooms_resp };

            Ok(warp::reply::with_status(
                warp::reply::json(&resp),
                StatusCode::OK,
            ))
        }
        Err(e) => Ok(warp::reply::with_status(
            warp::reply::json(&INTERNAL_ERROR_RESPONSE),
            StatusCode::INTERNAL_SERVER_ERROR,
        )),
    };
}

async fn login(
    login: Login,
    repository: Arc<Mutex<Box<dyn Repository>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let gen = uuid::Uuid::new_v4();
    debug!("random uuid: {}", gen);

    let repo = repository.lock().await;
    let room = repo.room();

    let auth_res = room.authorize(login.room_name.as_str(), login.password);
    let success = match auth_res {
        Ok(r) => r,
        Err(DBError {
            err_type: ErrorType::InvalidParams,
        }) => {
            error!("invalid params");
            return Ok(warp::reply::with_status(
                warp::reply::json(&WRONG_PARAMS_RESPONSE),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
        Err(e) => {
            error!("error authorizing DB: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&INTERNAL_ERROR_RESPONSE),
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ));
        }
    };

    if !success {
        return Ok(warp::reply::with_status(
            warp::reply::json(&FORBIDDEN_ERROR_RESPONSE),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }

    let uuid_string = gen.to_hyphenated().to_string();

    let token_r = repo.token();
    match token_r.insert(TokenData {
        room_name: login.room_name.as_str(),
        token: uuid_string.as_str(),
    }) {
        Ok(_) => {}
        Err(e) => {
            error!("error inserting token to DB: {}", e);
            return Ok(warp::reply::with_status(
                warp::reply::json(&INTERNAL_ERROR_RESPONSE),
                warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            ));
        }
    }

    Ok(warp::reply::with_status(
        warp::reply::json(&uuid_string.as_str()),
        warp::http::StatusCode::OK,
    ))
}

#[derive(Deserialize)]
pub struct Room {
    name: String,
    password: Option<String>,
    keywords: Option<Vec<String>>,
    description: Option<String>,
}

impl fmt::Display for Room {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let password = match &self.password {
            Some(pas) => pas.as_str(),
            None => "null",
        };

        write!(f, "name: {} password: {}", self.name, password)
    }
}

// must be used wit tls in production
async fn add_room(
    room_req: Room,
    repository: Arc<Mutex<Box<dyn Repository>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let repo = repository.lock().await;
    let room = repo.room();

    let password = room_req.password;

    let rm = RoomData {
        name: room_req.name.clone(),
        password,
        keywords: room_req.keywords,
        description: room_req.description,
    };

    let resp = match room.insert(rm) {
        Ok(_) => {
            info!("room with name '{}' has been added", room_req.name);
            reply::with_status(reply::json(&String::new()), StatusCode::OK)
        }
        Err(DBError {
            err_type: ErrorType::EntryExists,
        }) => {
            error!("room with name {} already exists", room_req.name);
            reply::with_status(reply::json(&ENTRY_EXISTS_RESPONSE), StatusCode::BAD_REQUEST)
        }
        Err(e) => {
            error!("{}", e);
            reply::with_status(
                reply::json(&INTERNAL_ERROR_RESPONSE),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    };

    Ok(resp)
}
