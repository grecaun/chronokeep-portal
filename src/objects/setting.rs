use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct Setting {
    name: String,
    value: String,
}

impl Setting {
    pub fn new(name: String, value: String) -> Setting {
        Setting {
            name,
            value
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}