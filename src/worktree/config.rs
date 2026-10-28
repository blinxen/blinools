use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub types: Vec<String>,
    pub wip_prefix: String,
    pub review_prefix: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            types: vec![String::from("task"), String::from("bug")],
            wip_prefix: String::from("wip-pr"),
            review_prefix: String::from("pr-review"),
        }
    }
}
