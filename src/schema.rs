// @generated automatically by Diesel CLI.

diesel::table! {
    tds_readings (id) {
        id -> Int4,
        tds_ppm -> Float8,
        timestamp -> Int8,
    }
}
