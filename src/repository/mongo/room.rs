use crate::repository::{DBError, ErrorType, Room};
use bcrypt::{hash, verify, DEFAULT_COST};
use mongodb::{
    bson::{doc, Bson, Document},
    error,
    sync::Client as MongoClient,
};
use std::borrow::Borrow;

use super::super::RoomData;

const DB_NAME: &str = "chat";
const COLLECTION_NAME: &str = "room";

const NAME_FIELD: &str = "name";
const KEYWORDS_FIELD: &str = "keywords";
const BCRYPT_PASS_FIELD: &str = "bcrypt_pass";
const DESCRIPTION_FIELD: &str = "description";

pub struct MongoRoom {
    collection: mongodb::sync::Collection,
}

impl MongoRoom {
    pub fn new(client: MongoClient) -> MongoRoom {
        let database = client.database(DB_NAME);
        let collection = database.collection(COLLECTION_NAME);

        MongoRoom { collection }
    }
}

impl Room for MongoRoom {
    fn authorize(&self, room_name: &str, password: Option<String>) -> Result<bool, DBError> {
        let doc_res = self.collection.find_one(doc! {NAME_FIELD: room_name}, None);
        let doc_opt = match doc_res {
            Ok(doc_opt) => doc_opt,
            Err(e) => {
                error!("{}", e);
                return Err({
                    DBError {
                        err_type: ErrorType::Other,
                    }
                });
            }
        };
        let doc = match doc_opt {
            Some(d) => d,
            None => {
                info!("failed authorize for room: {}", room_name);
                return Ok(false);
            }
        };

        let bcrypt_pass = match doc.get(BCRYPT_PASS_FIELD).and_then(Bson::as_str) {
            Some(b_pass) => {
                if password.is_none() {
                    // there is password in DB, but there is no password in param
                    return Err(DBError {
                        err_type: ErrorType::InvalidParams,
                    });
                }

                b_pass
            }
            None => return Ok(true),
        };

        let b_res = verify(password.unwrap(), bcrypt_pass); // we verified that password is not None above.
        let res = match b_res {
            Ok(r) => Ok(r),
            Err(e) => {
                error!("auth error: {}", e);
                Result::Err(DBError {
                    err_type: ErrorType::Other,
                })
            }
        };

        res
    }

    fn find(&self, keywords: Vec<&str>) -> Result<Vec<RoomData>, DBError> {
        let mut opt: Option<Document> = None;
        let keywords_len = keywords.len();
        if keywords_len > 1 || keywords_len == 1 && keywords[0] != "" {
            opt = Some(doc! {KEYWORDS_FIELD: {"$in":keywords}});
        }

        let mut cur = match self.collection.find(opt, None) {
            Ok(cur) => cur,
            Err(e) => {
                error!("{}", e);
                return Err({
                    DBError {
                        err_type: ErrorType::Other,
                    }
                });
            }
        };

        let mut res: Vec<RoomData> = Vec::new();

        while let Some(result) = cur.next() {
            match result {
                Ok(document) => {
                    let name = document.get(NAME_FIELD).and_then(Bson::as_str).unwrap(); // name field is required
                    let pass = document.get(BCRYPT_PASS_FIELD).and_then(Bson::as_str);
                    let keywords_opt = document.get(KEYWORDS_FIELD).and_then(Bson::as_array);
                    let description_opt = document.get(DESCRIPTION_FIELD).and_then(Bson::as_str);

                    let keywords: Option<Vec<String>> = match keywords_opt {
                        Some(keywords_bson) => {
                            let mut keywords: Vec<String> = Vec::new();

                            for v in keywords_bson {
                                let word = v.as_str().unwrap();
                                let word = word.to_string();
                                keywords.push(word)
                            }

                            Some(keywords)
                        }
                        None => None,
                    };

                    let room_data = RoomData {
                        name: name.to_owned(),
                        password: convert_option_string(pass),
                        keywords,
                        description: convert_option_string(description_opt),
                    };
                    res.push(room_data);
                }
                Err(e) => {
                    error!("{}", e);
                    return Err({
                        DBError {
                            err_type: ErrorType::Other,
                        }
                    });
                }
            };
        }

        Ok(res)
    }

    fn insert(&self, room_data: RoomData) -> Result<(), DBError> {
        let hashed_password: Bson = match room_data.password {
            Some(password) => match hash(password, DEFAULT_COST) {
                Ok(hashed_p) => Bson::String(hashed_p),
                Err(e) => {
                    error!("bcrypt error: {}", e);
                    return Err(DBError {
                        err_type: ErrorType::Other,
                    });
                }
            },
            None => Bson::Null,
        };

        let res = self.collection.insert_one(
            doc! {
            NAME_FIELD: room_data.name.clone(),
            BCRYPT_PASS_FIELD: hashed_password,
            KEYWORDS_FIELD: extract_option(room_data.keywords),
            DESCRIPTION_FIELD: extract_option(room_data.description)
            },
            None,
        );
        return match res {
            Ok(_) => {
                info!("room {} has been added", room_data.name);
                Ok(())
            }
            Err(e) => {
                error!("insert room error: {}", e);

                if let error::ErrorKind::WriteError(error::WriteFailure::WriteError(
                    error::WriteError { code: c, .. },
                )) = e.kind.borrow()
                {
                    return Err(DBError {
                        err_type: ErrorType::EntryExists,
                    });
                }

                return Err(DBError {
                    err_type: ErrorType::Other,
                });
            }
        };
    }
}

fn convert_option_string(input: Option<&str>) -> Option<String> {
    match input {
        Some(s) => Some(s.to_owned()),
        None => None,
    }
}

fn extract_option<V: Into<Bson>>(bson: Option<V>) -> Bson {
    match bson {
        Some(b) => b.into(),
        None => Bson::Null,
    }
}
