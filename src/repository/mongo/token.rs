use crate::repository::{DBError, ErrorType, Token, TokenData};
use chrono::prelude::Utc;
use mongodb::{bson::doc, sync::Client as MongoClient};

const DB_NAME: &str = "chat";
const COLLECTION_NAME: &str = "token";

const TOKEN_FIELD: &str = "token";
const ROOM_NAME_FIELD: &str = "room_name";
const VALID_TILL_FIELD: &str = "valid_till";

pub struct MongoToken {
    collection: mongodb::sync::Collection,
}

const TOKEN_LIFETIME_MINUTES: i64 = 1;

impl MongoToken {
    pub fn new(client: MongoClient) -> MongoToken {
        let database = client.database(DB_NAME);
        let collection = database.collection(COLLECTION_NAME);

        MongoToken { collection }
    }
}

impl Token for MongoToken {
    fn insert(&self, token: TokenData) -> Result<(), DBError> {
        let expire = Utc::now()
            .checked_add_signed(chrono::Duration::minutes(TOKEN_LIFETIME_MINUTES))
            .unwrap(); // token is valid for 1 minute

        let res = self.collection.insert_one(
            doc! {
            TOKEN_FIELD:token.token,
            ROOM_NAME_FIELD: token.room_name,
            VALID_TILL_FIELD:expire,
              },
            None,
        );
        return match res {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("token insertion error: {}", e);
                Err(DBError {
                    err_type: ErrorType::Other,
                })
            }
        };
    }

    fn delete(&self, token: TokenData) -> Result<(), DBError> {
        let filter = doc! {TOKEN_FIELD: token.token, ROOM_NAME_FIELD: token.room_name};
        let del_res = self.collection.delete_one(filter, None);

        match del_res {
            Ok(res) => {
                if res.deleted_count != 1 {
                    warn!("token deletion failed for room: {}", token.room_name)
                }

                Ok(())
            }
            Err(e) => {
                error!("token deletion error: {}", e);
                return Err({
                    DBError {
                        err_type: ErrorType::Other,
                    }
                });
            }
        }
    }

    fn get_valid(&self, token: TokenData) -> Result<bool, DBError> {
        let now = Utc::now();
        let doc_res = self.collection.find_one(
            doc! {TOKEN_FIELD: token.token, ROOM_NAME_FIELD: token.room_name, VALID_TILL_FIELD:{"$gte": now}},
            None,
        );

        let dc = match doc_res {
            Ok(d) => d,
            Err(e) => {
                error!("get_valid err: {}", e);
                return Err({
                    DBError {
                        err_type: ErrorType::Other,
                    }
                });
            }
        };

        match dc {
            Some(_dc) => Ok(true),
            None => Ok(false),
        }
    }
}
