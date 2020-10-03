use std::fmt;

pub mod mongo;

pub trait Repository: Send {
    fn token(&self) -> Box<dyn Token>;
    fn room(&self) -> Box<dyn Room>;
    fn message(&self) -> Box<dyn Message>;
}

#[derive(Deserialize, Serialize)]
pub struct RoomData {
    pub name: String,
    pub password: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub description: Option<String>,
}

pub struct TokenData<'b> {
    pub token: &'b str,
    pub room_name: &'b str,
}

pub struct MsgParams {
    pub page: i64,
    pub room_name: String,
    pub size: i64,
}

pub struct MessageData {
    pub room_name: String,
    pub user_name: String,
    pub message: String,
}

pub fn new_repo<'a>(
    database: &str,
    params: impl Into<DBParams>,
) -> Result<Box<dyn Repository>, DBError> {
    match database {
        "mongo" => {
            let r = mongo::MongoRepository::new(params)?;
            Ok(Box::new(r))
        }

        _ => Err(DBError {
            err_type: ErrorType::UnknownDBType,
        }),
    }
}

#[derive(Clone)]
pub struct DBParams {
    pub user_name: String,
    pub password: String,
    pub database: String,
    pub host: String,
    pub port: String,
}

pub trait Token {
    fn insert(&self, token: TokenData) -> Result<(), DBError>;
    fn delete(&self, token: TokenData) -> Result<(), DBError>;
    fn get_valid(&self, token: TokenData) -> Result<bool, DBError>;
}

pub trait Room {
    fn authorize(&self, room_name: &str, password: Option<String>) -> Result<bool, DBError>;
    fn find(&self, keywords: Vec<&str>) -> Result<Vec<RoomData>, DBError>;
    fn insert(&self, chat: RoomData) -> Result<(), DBError>;
}

pub trait Message {
    fn insert(&self, message: MessageData) -> Result<(), DBError>;
    fn get(&self, params: MsgParams) -> Result<Vec<MessageData>, DBError>;
}

#[derive(Debug)]
pub struct DBError {
    pub(crate) err_type: ErrorType,
}

impl fmt::Display for DBError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error type: {}", self.err_type)
    }
}

#[derive(Debug)]
pub enum ErrorType {
    Connection,
    Config,
    UnknownDBType,
    EntryExists,
    InconsistentState,
    InvalidParams,
    Other,
}

impl fmt::Display for ErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ErrorType::Connection => "connection",
            ErrorType::Config => "config",
            ErrorType::UnknownDBType => "this db is not implemented",
            ErrorType::EntryExists => "such key already exists",
            ErrorType::InconsistentState => "some values are wrong",
            ErrorType::InvalidParams => "supplied params are invalid",
            ErrorType::Other => "other",
        };
        write!(f, "Error type: {}", s)
    }
}
