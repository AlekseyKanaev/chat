use crate::repository::{DBError, ErrorType, Message, MessageData, MsgParams};
use chrono::prelude::Utc;
use mongodb::{
    bson::{doc, Bson, Document},
    options::FindOptions,
    sync::Client as MongoClient,
};
use serde::export::Formatter;
use std::fmt;

impl fmt::Display for MessageData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "room_name: {} user_name: {} message: {}",
            self.room_name.as_str(),
            self.user_name.as_str(),
            self.message.as_str()
        )
    }
}

const DB_NAME: &str = "chat";
const COLLECTION_NAME: &str = "message";

const ROOM_NAME_FIELD: &str = "room_name";
const USER_NAME_FIELD: &str = "user_name";
const MESSAGE_FIELD: &str = "message";
const CREATED_AT_FIELD: &str = "created_at";

pub struct MongoMessage {
    collection: mongodb::sync::Collection,
}

impl MongoMessage {
    pub fn new(client: MongoClient) -> MongoMessage {
        let database = client.database(DB_NAME);
        let collection = database.collection(COLLECTION_NAME);

        MongoMessage { collection }
    }
}

impl Message for MongoMessage {
    fn insert(&self, message: MessageData) -> Result<(), DBError> {
        let created_at = Utc::now();

        let res = self.collection.insert_one(
            doc! {
            ROOM_NAME_FIELD:  message.room_name.as_str(),
            USER_NAME_FIELD:  message.user_name.as_str(),
            MESSAGE_FIELD:    message.message.as_str(),
            CREATED_AT_FIELD: created_at.clone(),
              },
            None,
        );
        return match res {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("failed to insert message {}", message);
                Err(DBError {
                    err_type: ErrorType::Other,
                })
            }
        };
    }

    fn get(&self, params: MsgParams) -> Result<Vec<MessageData>, DBError> {
        let mut sort_opt = Document::new();
        sort_opt.insert(CREATED_AT_FIELD, Bson::Int32(-1)); // DESC
        let opt = FindOptions::builder().
            skip(params.size * params.page).
            limit(params.size).
            sort(sort_opt). // desc order
            build();
        let cur_res = self
            .collection
            .find(doc! {ROOM_NAME_FIELD: params.room_name}, opt);
        let mut cur = match cur_res {
            Ok(cur) => cur,
            Err(e) => {
                error!("get message error: {}", e);
                return Result::Err(DBError {
                    err_type: ErrorType::Other,
                });
            }
        };

        let mut res: Vec<MessageData> = Vec::new();
        while let Some(result) = cur.next() {
            match result {
                Ok(document) => {
                    let room_name_res = document.get(ROOM_NAME_FIELD).and_then(Bson::as_str);
                    let room_name = match room_name_res {
                        Some(r) => r.to_owned(),
                        None => {
                            error!(
                                "inconsistent state of db. {} field must be present",
                                ROOM_NAME_FIELD
                            );
                            return Result::Err(DBError {
                                err_type: ErrorType::InconsistentState,
                            });
                        }
                    };
                    let user_name_res = document.get(USER_NAME_FIELD).and_then(Bson::as_str);
                    let user_name = match user_name_res {
                        Some(r) => r.to_owned(),
                        None => {
                            error!(
                                "inconsistent state of db. {} field must be present",
                                USER_NAME_FIELD
                            );
                            return Result::Err(DBError {
                                err_type: ErrorType::InconsistentState,
                            });
                        }
                    };
                    let message_res = document.get(MESSAGE_FIELD).and_then(Bson::as_str);
                    let message = match message_res {
                        Some(r) => r.to_owned(),
                        None => {
                            error!(
                                "inconsistent state of db. {} field must be present",
                                MESSAGE_FIELD
                            );
                            return Result::Err(DBError {
                                err_type: ErrorType::InconsistentState,
                            });
                        }
                    };

                    let message_data = MessageData {
                        room_name,
                        user_name,
                        message,
                    };
                    res.push(message_data);
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
}
