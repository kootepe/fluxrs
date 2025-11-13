use egui::{Color32, RichText};
pub fn good_message(msg: &str) -> RichText {
    RichText::new(msg).color(Color32::GREEN)
}

pub fn bad_message(msg: &str) -> RichText {
    RichText::new(msg).color(Color32::RED)
}

pub fn warn_message(msg: &str) -> RichText {
    RichText::new(msg).color(Color32::YELLOW)
}
