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
    sqlx::query("SELECT uid FROM photo;")
        .fetch_all(db_conn).await
        .and_then(|rows| Ok(
            // Convert the list of rows into a list of UID's, excluding invalid inputs from the result
            rows.iter()
                .filter_map(|row| -> Option<UID> {
                    row.try_get(0).ok().and_then(|col: String| UID::try_from(&col).ok())
                })
                .collect::<Vec<UID>>()
        ))
        .map_err(|e| Error::DatabaseError(e))
}


/// Get the list of unique paths known in the database that start with the given path
pub async fn get_paths_starting_with(db_conn: &mut PoolConnection<Sqlite>, path: &PathBuf) -> Result<Vec<PathBuf>, Error> {
    let path_str = path.to_str().ok_or_else(|| Error::InvalidRequestError(path.clone()))?;

    sqlx::query("SELECT path FROM photo WHERE SUBSTR(path, 1, ?)=? GROUP BY path;")
        .bind(path_str.chars().count() as u32)
        .bind(path_str)
        .fetch_all(db_conn).await
        .and_then(|rows| Ok(
            // Convert the list of rows into a list of PathBuf's, excluding invalid inputs from the result
            rows.iter()
                .filter_map(|row| -> Option<PathBuf> {
                    row.try_get(0).ok().and_then(|col: String| PathBuf::try_from(&col).ok())
                })
                .collect::<Vec<PathBuf>>()
        ))
        .map_err(|e| Error::DatabaseError(e))
}


/// Get the list of photos known in the database that are registered in one of the given paths
pub async fn get_photos_in_paths(db_conn: &mut PoolConnection<Sqlite>, paths: &Vec<PathBuf>) -> Result<Vec<Photo>, Error> {
    let mut photos: Vec<Photo> = Vec::new();
    for batch in paths.chunks(100) {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM photo WHERE path IN (");
        let mut separated = query_builder.separated(", ");
        for path in batch {
            separated.push_bind(path.to_str().ok_or_else(|| Error::InvalidRequestError(path.clone()))?);
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
    query_builder.push_bind(path.to_str().ok_or_else(|| Error::InvalidRequestError(path.clone()))?);
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
        ))
        .map_err(|e| Error::DatabaseError(e))
}


/// Get a single photo based on its UID
pub async fn get_photo(db_conn: &mut PoolConnection<Sqlite>, uid: &UID) -> Result<Option<Photo>, Error> {
    sqlx::query("SELECT * FROM photo WHERE uid=? LIMIT 1;")
        .bind(uid.to_string())
        .try_map(|row: SqliteRow| -> Result<Photo, sqlx::Error> {
            row_to_photo(&row)
                .or_else(|e| {
                    eprintln!("Warning : database error : unable to decode a photo : {}", e);
                    Err(e)
                })
        })
        .fetch_optional(db_conn).await
        .map_err(|e| Error::DatabaseError(e))
}


/// Insert a list of photos into the database
pub async fn insert_photos(db_conn: &mut PoolConnection<Sqlite>, photos: &Vec<Photo>) -> Result<(), Error> {
    // Insert photos by batches of up to 100
    for batch in photos.chunks(100) {
        let mut query_builder = QueryBuilder::new("INSERT INTO photo(filename, path, uid, md5) ");
        query_builder.push_values(batch, |mut builder, photo| {
            builder.push_bind(&photo.filename)
                .push_bind(photo.path.to_str().unwrap())
                .push_bind(photo.uid.to_string())
                .push_bind(&photo.md5);
        });
        let query = query_builder.build();
        query.execute(&mut *db_conn).await
            .map_err(|e| Error::DatabaseError(e))?;
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
        query.execute(&mut *db_conn).await
            .map_err(|e| Error::DatabaseError(e))?;
    }
    Ok(())
}


/// Rename/move a list of photos in the database, based on their UIDs
pub async fn move_photos(db_conn: &mut PoolConnection<Sqlite>, photos_pairs: &Vec<(Photo, Photo)>) -> Result<(), Error> {
    for photos_pair in photos_pairs {
        sqlx::query("UPDATE photo SET filename=?, path=? WHERE uid=?;")
            .bind(&photos_pair.1.filename)
            .bind(&photos_pair.1.path.to_str().unwrap())
            .bind(&photos_pair.0.uid.to_string())
            .execute(&mut *db_conn).await
            .map_err(|e| Error::DatabaseError(e))?;
    }
    Ok(())
}


/// Update a photo in the database based on its UID
pub async fn update_photo(db_conn: &mut PoolConnection<Sqlite>, photo: &Photo) -> Result<(), Error> {
    let sql = "
        UPDATE photo SET
            filename=?,
            path=?,
            md5=?,
            sort_order=?,
            hidden=?,
            metadata_parsed=?,
            width=?,
            height=?,
            color=?,
            title=?,
            place=?,
            date_taken=?,
            camera_model=?,
            lens_model=?,
            focal_length=?,
            aperture=?,
            exposure_time=?,
            sensitivity=?
        WHERE uid=?;
    ";
    sqlx::query(sql)
        .bind(&photo.filename)
        .bind(photo.path.to_str().unwrap())
        .bind(&photo.md5)
        .bind(photo.sort_order)
        .bind(photo.hidden)
        .bind(photo.metadata_parsed)
        .bind(photo.width)
        .bind(photo.height)
        .bind(&photo.color)
        .bind(&photo.title)
        .bind(&photo.place)
        .bind(&photo.date_taken)
        .bind(&photo.camera_model)
        .bind(&photo.lens_model)
        .bind(&photo.focal_length)
        .bind(&photo.aperture)
        .bind(&photo.exposure_time)
        .bind(&photo.sensitivity)
        .bind(photo.uid.to_string())
        .execute(db_conn).await
        .map_err(|e| Error::DatabaseError(e))
        .and_then(|r| if r.rows_affected() > 0 {
            Ok(())
        } else {
            Err(Error::InvalidUIDError(photo.uid.clone()))
        })
}


/// Deserialize an SQL row into a Photo struct, based on the order defined in schema.sql
fn row_to_photo(row: &SqliteRow) -> Result<Photo, sqlx::Error> {
    Ok(Photo {
        id: row.try_get(0)?,
        filename: row.try_get(1)?,
        path: PathBuf::from(row.try_get::<String, _>(2)?),
        uid: UID::try_from(row.try_get::<&str, _>(3)?).unwrap(),
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
