use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub types: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            types: vec![String::from("task"), String::from("bug")],
        }
    }
}
