pub struct EnableFit {
    pub show_linfit: bool,
    pub show_polyfit: bool,
    pub show_roblinfit: bool,
    pub show_expfit: bool,
}

impl EnableFit {
    pub fn new() -> Self {
        Self { show_linfit: true, show_polyfit: true, show_roblinfit: true, show_expfit: true }
    }
}

impl Default for EnableFit {
    fn default() -> Self {
        Self::new()
    }
}
