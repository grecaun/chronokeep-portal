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
}