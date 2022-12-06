use std::path::PathBuf;

use crate::{Error, photos::Photo, uid::UID};
use rocket::{fairing, Rocket, Build, tokio::fs};
use rocket_db_pools::{sqlx::{self, query::Query, Sqlite, sqlite::{SqliteArguments, SqliteRow}, QueryBuilder, pool::PoolConnection}, sqlx::Row, Database};


#[derive(Database)]
#[database("niobium")]
pub struct DB(pub sqlx::SqlitePool);



/// Fairing callback that checks if the database has already been filled with the `photo`
/// table and if not, executes `schema.sql` to initialize it
pub async fn init_schema(rocket: Rocket<Build>) -> fairing::Result {
    // Make sure the database has been initialized (fairings have been attached in the correct order)
    if let Some(db) = DB::fetch(&rocket) {
        let db = &db.0;

        // Check the `sqlite_master` table for a table named `photo`
        let query_result = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='photo';")
            .fetch_optional(db).await;
        match query_result {
            // The table already exists, we can proceed with liftoff
            Ok(Some(_)) => Ok(rocket),

            // The table doesn't exist, try to import the schema to create it
            Ok(None) => {
                print!("Database is empty, creating schema... ");

                // Try to open `schema.sql`
                match fs::read_to_string("schema.sql").await {
                    Ok(schema) => {
                        // Split the schema to import into individual queries
                        let sql_queries = schema.split(';').map(|s| s.trim()).filter(|s| !s.is_empty());
                        for sql_query in sql_queries {
                            let query_result = sqlx::query(sql_query)
                                .execute(db).await;
                            if let Err(error) = query_result {
                                println!("");
                                eprintln!("Error, unable to execute a query from schema.sql :");
                                eprintln!("{}", sql_query);
                                eprintln!("Result : {}", error);
                                return Err(rocket);
                            }
                        }
                        println!("success");
                        Ok(rocket)
                    },
                    Err(error) => {
                        println!("");
                        eprintln!("Error, unable to open \"schema.sql\" : {}", error);
                        Err(rocket)
                    },
                }
            },

            // Something went wrong when checking `sqlite_master`, we'll have to scrub the launch
            Err(e) => {
                eprintln!("Error, unable to access database to check schema : {}", e);
                Err(rocket)
            }
        }
    } else {
        Err(rocket)
    }
}


/// Get the list of UIDs that exist in the database
pub async fn get_existing_uids(db_conn: &mut PoolConnection<Sqlite>) -> Result<Vec<UID>, Error> {
    Ok(
        sqlx::query("SELECT uid FROM photo;")
            .fetch_all(db_conn).await
            .and_then(|rows| Ok(
                // Convert the list of rows into a list of UID's, excluding invalid inputs from the result
                rows.iter()
                    .filter_map(|row| -> Option<UID> {
                        row.try_get(0).ok().and_then(|col: String| UID::try_from(&col).ok())
                    })
                    .collect::<Vec<UID>>()
            ))?
    )
}


/// Get the list of unique paths known in the database
pub async fn get_all_paths(db_conn: &mut PoolConnection<Sqlite>) -> Result<Vec<PathBuf>, Error> {
    Ok(
        sqlx::query("SELECT path FROM photo GROUP BY path;")
            .fetch_all(db_conn).await
            .and_then(|rows| Ok(
                // Convert the list of rows into a list of PathBuf's, excluding invalid inputs from the result
                rows.iter()
                    .filter_map(|row| -> Option<PathBuf> {
                        row.try_get(0).ok().and_then(|col: String| PathBuf::try_from(&col).ok())
                    })
                    .collect::<Vec<PathBuf>>()
            ))?
    )
}


/// Get the list of photos known in the database that are registered in one of the given paths
pub async fn get_photos_in_paths(db_conn: &mut PoolConnection<Sqlite>, paths: &Vec<PathBuf>) -> Result<Vec<Photo>, Error> {
    let mut photos: Vec<Photo> = Vec::new();
    for batch in paths.chunks(100) {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM photo WHERE path IN (");
        let mut separated = query_builder.separated(", ");
        for path in batch {
            separated.push_bind(path.to_string_lossy());
        }
        separated.push_unseparated(");");
        let query = query_builder.build();
        photos.append(&mut get_photos_from_query(db_conn, query).await?);
    }
    Ok(photos)
}


/// Get the list of photos known in the database that are registered in the given path, ordered
pub async fn get_photos_in_path(db_conn: &mut PoolConnection<Sqlite>, path: &PathBuf, sort_columns: &Vec<String>) -> Result<Vec<Photo>, Error> {
    let mut query_builder = QueryBuilder::new("SELECT * FROM photo WHERE path=");
    query_builder.push_bind(path.to_string_lossy());
    query_builder.push(" ORDER BY ");
    let mut separated = query_builder.separated(", ");
    for col in sort_columns {
        separated.push(col);
    }
    separated.push_unseparated(";");

    let query = query_builder.build();
    get_photos_from_query(db_conn, query).await
}


/// Execute the given query (which must be a "SELECT * FROM photo", and parameters must already have been bound)
/// and map the resulting rows to a list of Photo's
async fn get_photos_from_query<'q>(db_conn: &mut PoolConnection<Sqlite>, query: Query<'q, Sqlite, SqliteArguments<'q>>) -> Result<Vec<Photo>, Error> {
    Ok(
        query.fetch_all(db_conn).await
            .and_then(|rows| Ok(
                // Convert the list of rows into a list of Photo's, excluding invalid inputs from the result
                rows.iter()
                    .filter_map(|row| -> Option<Photo> {
                        row_to_photo(row).or_else(|e| {
                            eprintln!("Warning : database error : unable to decode a photo : {}", e);
                            Err(e)
                        }).ok()
                    })
                    .collect::<Vec<Photo>>()
            ))?
    )
}


/// Insert a list of photos into the database
pub async fn insert_photos(db_conn: &mut PoolConnection<Sqlite>, photos: &Vec<Photo>) -> Result<(), Error> {
    // Insert photos by batches of up to 100
    for batch in photos.chunks(100) {
        let mut query_builder = QueryBuilder::new("
            INSERT INTO photo(
                filename,
                path,
                uid,
                md5,
                sort_order,
                hidden,
                metadata_parsed,
                width,
                height,
                color,
                title,
                place,
                date_taken,
                camera_model,
                lens_model,
                focal_length,
                aperture,
                exposure_time,
                sensitivity
        ) ");
        query_builder.push_values(batch, |mut builder, photo| {
            builder
                .push_bind(&photo.filename)
                .push_bind(photo.path.to_string_lossy())
                .push_bind(photo.uid.to_string())
                .push_bind(&photo.md5)
                .push_bind(photo.sort_order)
                .push_bind(photo.hidden)
                .push_bind(photo.metadata_parsed)
                .push_bind(photo.width)
                .push_bind(photo.height)
                .push_bind(&photo.color)
                .push_bind(&photo.title)
                .push_bind(&photo.place)
                .push_bind(&photo.date_taken)
                .push_bind(&photo.camera_model)
                .push_bind(&photo.lens_model)
                .push_bind(&photo.focal_length)
                .push_bind(&photo.aperture)
                .push_bind(&photo.exposure_time)
                .push_bind(&photo.sensitivity);
        });
        let query = query_builder.build();
        query.execute(&mut *db_conn).await?;
    }
    Ok(())
}


/// Remove a list of photos from the database, based on their UIDs
pub async fn remove_photos(db_conn: &mut PoolConnection<Sqlite>, photos: &Vec<Photo>) -> Result<(), Error> {
    // Remove photos by batches of up to 100
    for batch in photos.chunks(100) {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new("DELETE FROM photo WHERE uid IN (");
        let mut separated = query_builder.separated(", ");
        for photo in batch {
            separated.push_bind(photo.uid.to_string());
        }
        separated.push_unseparated(");");
        let query = query_builder.build();
        query.execute(&mut *db_conn).await?;
    }
    Ok(())
}


/// Rename/move a list of photos in the database, based on their UIDs
pub async fn move_photos(db_conn: &mut PoolConnection<Sqlite>, photos_pairs: &Vec<(Photo, Photo)>) -> Result<(), Error> {
    for photos_pair in photos_pairs {
        sqlx::query("UPDATE photo SET filename=?, path=? WHERE uid=?;")
            .bind(&photos_pair.1.filename)
            .bind(&photos_pair.1.path.to_string_lossy())
            .bind(&photos_pair.0.uid.to_string())
            .execute(&mut *db_conn).await?;
    }
    Ok(())
}


/// Deserialize an SQL row into a Photo struct, based on the order defined in schema.sql
fn row_to_photo(row: &SqliteRow) -> Result<Photo, sqlx::Error> {
    Ok(Photo {
        id: row.try_get(0)?,
        filename: row.try_get(1)?,
        path: PathBuf::from(row.try_get::<String, _>(2)?),
        uid: UID::try_from(row.try_get::<&str, _>(3)?).unwrap_or(UID::empty()),
        md5: row.try_get(4)?,
        sort_order: row.try_get(5)?,
        hidden: row.try_get(6)?,
        metadata_parsed: row.try_get(7)?,
        width: row.try_get(8)?,
        height: row.try_get(9)?,
        color: row.try_get(10)?,
        title: row.try_get(11)?,
        place: row.try_get(12)?,
        date_taken: row.try_get(13)?,
        camera_model: row.try_get(14)?,
        lens_model: row.try_get(15)?,
        focal_length: row.try_get(16)?,
        aperture: row.try_get(17)?,
        exposure_time: row.try_get(18)?,
        sensitivity: row.try_get(19)?,
        ..Default::default()
    })
}
