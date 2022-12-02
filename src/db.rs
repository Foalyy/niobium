use std::path::PathBuf;

use crate::{config::Config, Error, photos::Photo, uid::UID};
use rocket::tokio::sync::Mutex;
use rusqlite::{Connection, OptionalExtension, Row};


/// Try to open the sqlite database used to store the photos information
/// If it does not exist, try to create it and initialize it with the default schema
/// In case of error, print it to stderr and exit with a status code of -1
pub fn open_or_exit(config: &Config) -> Connection {
    // Try to open the database
    // If it doesn't exist, an empty one will be created thanks to the default SQLITE_OPEN_CREATE flag
    let db_conn = Connection::open(&config.DATABASE_PATH).unwrap_or_else(|error| {
            eprintln!("Error, unable to open the database : {}", error);
            std::process::exit(-1);
    });

    // Check if the main 'photo' table exist, and if not, try to create it
    match db_conn.query_row(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='photo';",
        [], |row| row.get::<_, String>(0)
    ).optional() {
        Ok(result) => result.unwrap_or_else(|| {
            // The main table doesn't exist, import the default schema to initialize the database
            print!("Database is empty, creating schema... ");
            let schema = std::fs::read_to_string("schema.sql").unwrap_or_else(|error| {
                println!("");
                eprintln!("Error, unable to open \"schema.sql\" : {}", error);
                std::process::exit(-1);
            });
            db_conn.execute_batch(&schema).unwrap_or_else(|error| {
                println!("");
                eprintln!("Error, unable to open \"schema.sql\" : {}", error);
                std::process::exit(-1);
            });
            println!("ok");
            String::new()
        }),
        Err(error) => {
            eprintln!("Error, unable to read from the database : {}", error);
            std::process::exit(-1);
        }
    };

    db_conn
}


/// Get the list of UIDs that exist in the database
pub async fn get_existing_uids(db_conn: &Mutex<Connection>) -> Result<Vec<UID>, Error> {
    let db_guard = db_conn.lock().await;

    let sql = "SELECT uid FROM photo;";

    let mut stmt = db_guard.prepare(sql)
        .map_err(|e| Error::DatabaseError(e))?;

    let uids = stmt.query_map([], |row| row.get(0))
        .map_err(|e| Error::DatabaseError(e))?
        .map(|x| x.unwrap())
        .collect::<Vec<UID>>();
    
    Ok(uids)
}


/// Get the list of unique paths known in the database that start with the given path
pub async fn get_paths_starting_with(db_conn: &Mutex<Connection>, path: &PathBuf) -> Result<Vec<PathBuf>, Error> {
    let db_guard = db_conn.lock().await;

    let sql = "SELECT path FROM photo WHERE SUBSTR(path, 1, ?)=? GROUP BY path;";

    let mut stmt = db_guard.prepare(sql)
        .map_err(|e| Error::DatabaseError(e))?;

    let params = (path.to_str().unwrap().chars().count(), path.to_str().unwrap());

    let paths = stmt.query_map(params, |row| 
        Ok(PathBuf::from(row.get::<usize, String>(0)?))
    )
        .map_err(|e| Error::DatabaseError(e))?
        .map(|x| x.unwrap())
        .collect::<Vec<PathBuf>>();

    Ok(paths)
}


/// Get the list of photos known in the database that are registered in one of the given paths
pub async fn get_photos_in_paths(db_conn: &Mutex<Connection>, paths: &Vec<PathBuf>) -> Result<Vec<Photo>, Error> {
    let db_guard = db_conn.lock().await;

    let mut sql = "SELECT * FROM photo WHERE path IN (".to_string();
    for (i, _) in paths.iter().enumerate() {
        if i > 0 {
            sql += ",";
        }
        sql += "?";
    }
    sql += ");";

    let mut stmt = db_guard.prepare(sql.as_str())
        .map_err(|e| Error::DatabaseError(e))?;

    let params = rusqlite::params_from_iter(paths.iter().map(|p| p.to_str().unwrap()));
    
    let photos = stmt.query_map(params, |row|
        Ok(row_to_photo(row)?)
    )
        .map_err(|e| Error::DatabaseError(e))?
        .map(|x| x.unwrap())
        .collect::<Vec<Photo>>();
    
    Ok(photos)
}


/// Get the list of photos known in the database that are registered in the given path, ordered
pub async fn get_photos_in_path(db_conn: &Mutex<Connection>, path: &PathBuf, sort_columns: &Vec<String>) -> Result<Vec<Photo>, Error> {
    let db_guard = db_conn.lock().await;

    let mut sql = "SELECT * FROM photo WHERE path=? ORDER BY ".to_string();
    sql += sort_columns.iter()
        .map(|clause| clause.clone() + " ASC")
        .collect::<Vec<String>>()
        .join(", ")
        .as_str();
    sql += ";";

    let mut stmt = db_guard.prepare(sql.as_str())
        .map_err(|e| Error::DatabaseError(e))?;

    let params = rusqlite::params![&path.to_str().unwrap()];
    
    let photos = stmt.query_map(params, |row|
        Ok(row_to_photo(row)?)
    )
        .map_err(|e| Error::DatabaseError(e))?
        .map(|x| x.unwrap())
        .collect::<Vec<Photo>>();
    
    Ok(photos)
}


/// Get a single photo based on its UID
pub async fn get_photo(db_conn: &Mutex<Connection>, uid: &UID) -> Result<Option<Photo>, Error> {
    let db_guard = db_conn.lock().await;

    let sql = "SELECT * FROM photo WHERE uid=? LIMIT 1;";

    let params = rusqlite::params![uid];
    
    let photo = db_guard.query_row(sql, params, |row|
        Ok(row_to_photo(row)?)
    )
        .optional()
        .map_err(|e| Error::DatabaseError(e))?;
    
    Ok(photo)
}


/// Insert a list of photos into the database
pub async fn insert_photos(db_conn: &Mutex<Connection>, photos: &Vec<Photo>) -> Result<(), Error> {
    let db_guard = db_conn.lock().await;

    let sql = "INSERT INTO photo(filename, path, uid, md5) VALUES(?, ?, ?, ?);";

    let mut stmt = db_guard.prepare(sql)
        .map_err(|e| Error::DatabaseError(e))?;
    
    for photo in photos {
        let params = rusqlite::params![&photo.filename, &photo.path.to_str().unwrap(), &photo.uid, &photo.md5];
        stmt.execute(params)
            .map_err(|e| Error::DatabaseError(e))?;
    }

    stmt.finalize().map_err(|e| Error::DatabaseError(e))
}


/// Remove a list of photos from the database, based on their UIDs
pub async fn remove_photos(db_conn: &Mutex<Connection>, photos: &Vec<Photo>) -> Result<(), Error> {
    let db_guard = db_conn.lock().await;

    let sql = "DELETE FROM photo WHERE uid=?;";

    let mut stmt = db_guard.prepare(sql)
        .map_err(|e| Error::DatabaseError(e))?;
    
    for photo in photos {
        stmt.execute(rusqlite::params![&photo.uid])
            .map_err(|e| Error::DatabaseError(e))?;
    }

    stmt.finalize().map_err(|e| Error::DatabaseError(e))
}


/// Rename/move a list of photos in the database, based on their UIDs
pub async fn move_photos(db_conn: &Mutex<Connection>, photos_pairs: &Vec<(Photo, Photo)>) -> Result<(), Error> {
    let db_guard = db_conn.lock().await;

    let sql = "UPDATE photo SET filename=?, path=? WHERE uid=?;";

    let mut stmt = db_guard.prepare(sql)
        .map_err(|e| Error::DatabaseError(e))?;
    
    for photos_pair in photos_pairs {
        stmt.execute(rusqlite::params![&photos_pair.1.filename, &photos_pair.1.path.to_str().unwrap(), &photos_pair.0.uid])
            .map_err(|e| Error::DatabaseError(e))?;
    }

    stmt.finalize().map_err(|e| Error::DatabaseError(e))
}


/// Update a photo in the database based on its UID
pub async fn update_photo(db_conn: &Mutex<Connection>, photo: &Photo) -> Result<(), Error> {
    let db_guard = db_conn.lock().await;

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

    let mut stmt = db_guard.prepare(sql)
        .map_err(|e| Error::DatabaseError(e))?;
    
    let params = rusqlite::params![
        photo.filename,
        photo.path.to_str().unwrap(),
        photo.md5,
        photo.sort_order,
        photo.hidden,
        photo.metadata_parsed,
        photo.width,
        photo.height,
        photo.color,
        photo.title,
        photo.place,
        photo.date_taken,
        photo.camera_model,
        photo.lens_model,
        photo.focal_length,
        photo.aperture,
        photo.exposure_time,
        photo.sensitivity,
        photo.uid,
    ];
    
    stmt.execute(params)
        .map_err(|e| Error::DatabaseError(e))?;

    stmt.finalize().map_err(|e| Error::DatabaseError(e))
}


/// Deserialize an SQL row into a Photo struct, based on the order defined in schema.sql
fn row_to_photo(row: &Row) -> rusqlite::Result<Photo> {
    Ok(Photo {
        id: row.get(0)?,
        filename: row.get(1)?,
        path: PathBuf::from(row.get::<usize, String>(2)?),
        uid: row.get(3)?,
        md5: row.get(4)?,
        sort_order: row.get(5)?,
        hidden: row.get(6)?,
        metadata_parsed: row.get(7)?,
        width: row.get(8)?,
        height: row.get(9)?,
        color: row.get(10)?,
        title: row.get(11)?,
        place: row.get(12)?,
        date_taken: row.get(13)?,
        camera_model: row.get(14)?,
        lens_model: row.get(15)?,
        focal_length: row.get(16)?,
        aperture: row.get(17)?,
        exposure_time: row.get(18)?,
        sensitivity: row.get(19)?,
        ..Default::default()
    })
}
