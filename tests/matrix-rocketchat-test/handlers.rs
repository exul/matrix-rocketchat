use iron::prelude::*;
use iron::status;

pub fn matrix_version(_request: &mut Request) -> IronResult<Response> {
    let payload = r#"{"versions":["r0.0.1","r0.1.0","r0.2.0"]}"#;
    Ok(Response::with((status::Ok, payload)))
}

pub fn empty_json(_request: &mut Request) -> IronResult<Response> {
    Ok(Response::with((status::Ok, "{}")))
}
