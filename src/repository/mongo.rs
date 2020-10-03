pub mod message;
pub mod room;
pub mod token;

use super::{DBError, DBParams, ErrorType, Message, Repository, Room, Token};
use mongodb::sync::Client as MongoClient;

pub struct MongoRepository {
    client: MongoClient,
}

impl Repository for Box<MongoRepository> {
    fn token(&self) -> Box<dyn Token> {
        let t = token::MongoToken::new(self.client.clone());

        Box::new(t)
    }

    fn room(&self) -> Box<dyn Room> {
        let r = room::MongoRoom::new(self.client.clone());

        Box::new(r)
    }

    fn message(&self) -> Box<dyn Message> {
        let m = message::MongoMessage::new(self.client.clone());

        Box::new(m)
    }
}

impl MongoRepository {
    pub fn new(params: impl Into<DBParams>) -> Result<Box<MongoRepository>, DBError> {
        let params: DBParams = params.into();
        let connection_string = format!(
            "mongodb://{}:{}@{}:{}",
            params.user_name, params.password, params.host, params.port
        );

        let client_res = MongoClient::with_uri_str(connection_string.as_str());
        let client = match client_res {
            Ok(c) => c,
            Err(e) => {
                return Err(DBError {
                    err_type: ErrorType::Config,
                });
            } // todo: log error
        };

        // connection test
        match client.list_database_names(None, None) {
            Ok(_) => {} // todo: log
            Err(e) => {
                return Err(DBError {
                    err_type: ErrorType::Connection,
                });
            } // todo: log error
        }

        Ok(Box::new(MongoRepository { client }))
    }
}
