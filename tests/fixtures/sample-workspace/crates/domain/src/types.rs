/// A monetary amount.
pub struct Price {
    pub amount: f64,
    pub currency: String,
}

pub trait CurrencyFormat {
    fn format_idr(&self) -> String;
}

impl CurrencyFormat for Price {
    fn format_idr(&self) -> String {
        format!("IDR {:.0}", self.amount)
    }
}

pub fn calculate_fee(amount: f64, rate: f64) -> f64 {
    amount * rate
}
