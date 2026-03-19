use domain::prelude::Price;

pub struct OrderRepository;

impl OrderRepository {
    pub fn find_total(&self) -> Price {
        Price { amount: 100000.0, currency: "IDR".to_string() }
    }
}

macro_rules! log_query {
    ($q:expr) => {
        println!("Query: {}", $q);
    };
}
