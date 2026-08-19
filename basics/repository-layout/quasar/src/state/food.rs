/// A carnival food stand.
pub struct FoodStand {
    pub name: &'static str,
    pub food_type: &'static str,
    pub tickets: u32,
}

pub fn food_stand_name_matches(stand: &FoodStand, other: &str) -> bool {
    stand.name.as_bytes() == other.as_bytes()
}

/// Static list of food stands.
pub fn get_food_stands() -> &'static [FoodStand] {
    &[
        FoodStand {
            name: "Larry's Pizza",
            food_type: "pizza",
            tickets: 3,
        },
        FoodStand {
            name: "Taco Shack",
            food_type: "taco",
            tickets: 2,
        },
        FoodStand {
            name: "Dough Boy's",
            food_type: "fried dough",
            tickets: 1,
        },
    ]
}
