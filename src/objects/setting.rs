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

    pub fn Name(&self) -> &String {
        &self.name
    }

    pub fn Value(&self) -> &String {
        &self.value
    }
}