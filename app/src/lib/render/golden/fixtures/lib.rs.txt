//! The app's commands.
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Batch<'a> {
    pub id: &'a str,
    tries: u32,
}

impl<'a> Batch<'a> {
    pub fn settle(&mut self) -> Result<String, String> {
        let mut seen: HashMap<&str, u32> = HashMap::new();
        self.tries += 1;
        if self.tries > 3 { return Err(format!("parked {}", self.id)); }
        seen.insert(self.id, self.tries);
        Ok(String::from("sent\n"))
    }
}
