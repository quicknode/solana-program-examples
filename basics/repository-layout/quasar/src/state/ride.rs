/// A carnival ride with requirements.
/// Uses &'static str instead of String for no_std compatibility.
pub struct Ride {
    pub name: &'static str,
    pub upside_down: bool,
    pub tickets: u32,
    pub min_height: u32,
}

/// Check if a ride name matches (byte comparison, no alloc).
pub fn ride_name_matches(ride: &Ride, other: &str) -> bool {
    ride.name.as_bytes() == other.as_bytes()
}

/// Static list of carnival rides.
pub fn get_rides() -> &'static [Ride] {
    &[
        Ride {
            name: "Tilt-a-Whirl",
            upside_down: false,
            tickets: 3,
            min_height: 48,
        },
        Ride {
            name: "Scrambler",
            upside_down: false,
            tickets: 3,
            min_height: 48,
        },
        Ride {
            name: "Ferris Wheel",
            upside_down: false,
            tickets: 5,
            min_height: 55,
        },
        Ride {
            name: "Zero Gravity",
            upside_down: true,
            tickets: 5,
            min_height: 60,
        },
    ]
}
