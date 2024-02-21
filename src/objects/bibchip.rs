use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BibChip {
    bib: String,
    chip: String,
}

impl BibChip {
    pub fn new(
        bib: String,
        chip: String,
    ) -> BibChip {
        BibChip {
            bib,
            chip,
        }
    }

    pub fn bib(&self) -> &str {
        &self.bib
    }

    pub fn chip(&self) -> &str {
        &self.chip
    }

    pub fn equals(&self, other: &BibChip) -> bool {
        return self.bib == other.bib
            && self.chip == other.chip;
    }
}