use crate::http_server::{Params as http_params, Params};
use crate::repository::DBParams;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub db: DBConfig,
    pub http: Http,
    pub ws_url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DBConfig {
    host: String,
    port: String,
    database: String,
    user: String,
    password: String,
}

impl Into<DBParams> for DBConfig {
    fn into(self) -> DBParams {
        DBParams {
            user_name: self.user,
            password: self.password,
            database: self.database,
            host: self.host,
            port: self.port,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Http {
    ip: String,
    port: u16,
}

// It will panic if string has invalid format
impl Into<http_params> for Http {
    fn into(self) -> Params {
        let octates: Vec<u8> = self.ip.split(".").map(|s| s.parse().unwrap()).collect();

        let ip_address: [u8; 4] = [octates[0], octates[1], octates[2], octates[3]];
        let port = self.port;

        Params { ip_address, port }
    }
}
