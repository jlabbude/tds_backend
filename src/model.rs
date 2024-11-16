use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::tds_readings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TDS {
    pub id: i32,
    pub tds_ppm: f64,
    pub timestamp: i64,
}
