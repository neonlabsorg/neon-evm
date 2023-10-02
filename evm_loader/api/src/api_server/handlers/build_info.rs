use crate::build_info::get_build_info;
use actix_request_identifier::RequestId;
use actix_web::get;
use actix_web::http::StatusCode;
use actix_web::web::Json;
use actix_web::Responder;

#[tracing::instrument(skip(request_id), fields(id = request_id.as_str()), ret)]
#[get("/build-info")]
pub async fn build_info_route(request_id: RequestId) -> impl Responder {
    (Json(get_build_info()), StatusCode::OK)
}
