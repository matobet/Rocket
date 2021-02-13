#[macro_use] extern crate rocket;

mod paste_id;
#[cfg(test)] mod tests;

use std::io;

use rocket::State;
use rocket::data::{Data, ToByteUnit};
use rocket::http::uri::Absolute;
use rocket::response::content::Plain;
use rocket::tokio::fs::File;

use crate::paste_id::PasteId;

const HOST: &str = "http://localhost:8000";
const ID_LENGTH: usize = 3;

#[post("/", data = "<paste>")]
async fn upload(paste: Data, host: State<'_, Absolute<'_>>) -> io::Result<String> {
    let id = PasteId::new(ID_LENGTH);
    paste.open(128.kibibytes()).into_file(id.file_path()).await?;
    Ok(host.inner().clone().with_origin(uri!(retrieve: id)).to_string())
    // TODO: Ok(uri!(HOST, retrieve: id))
}

#[get("/<id>")]
async fn retrieve(id: PasteId<'_>) -> Option<Plain<File>> {
    File::open(id.file_path()).await.map(Plain).ok()
}

#[get("/")]
fn index() -> &'static str {
    "
    USAGE

      POST /

          accepts raw data in the body of the request and responds with a URL of
          a page containing the body's content

          EXAMPLE: curl --data-binary @file.txt http://localhost:8000

      GET /<id>

          retrieves the content for the paste with id `<id>`
    "
}

#[launch]
fn rocket() -> rocket::Rocket {
    rocket::ignite()
        .manage(Absolute::parse(HOST).expect("valid host"))
        .mount("/", routes![index, upload, retrieve])
}
