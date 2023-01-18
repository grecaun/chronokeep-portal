pub struct Participant {
    id: u64,
    bib: String,
    first: String,
    last: String,
    age: u16,
    gender: String,
    age_group: String,
    distance: String,
    chip: String,
    anonymous: bool,
}

impl Participant {
    pub fn new(
        id: u64,
        bib: String,
        first: String,
        last: String,
        age: u16,
        gender: String,
        age_group: String,
        distance: String,
        chip: String,
        anonymous: bool,
    ) -> Participant {
        Participant {
            id,
            bib,
            first,
            last,
            age,
            gender,
            age_group,
            distance,
            chip,
            anonymous,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn bib(&self) -> &str {
        &self.bib
    }

    pub fn first(&self) -> &str {
        &self.first
    }

    pub fn last(&self) -> &str {
        &self.last
    }

    pub fn age(&self) -> u16 {
        self.age
    }

    pub fn gender(&self) -> &str {
        &self.gender
    }

    pub fn age_group(&self) -> &str {
        &self.age_group
    }

    pub fn distance(&self) -> &str {
        &self.distance
    }

    pub fn chip(&self) -> &str {
        &self.chip
    }

    pub fn anonymous(&self) -> bool {
        self.anonymous
    }
}