use std::net::SocketAddr;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use tokio::net::TcpListener;
use bytes::Bytes;


type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;
type BoxedBody = BoxBody<Bytes, hyper::Error>;


#[tokio::main]
async fn main() -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).await?;
    println!("Relay listening on http://{}", addr);

    let client = Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(HttpConnector::new());

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let client_clone = client.clone();

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(move |req| {
                    relay_handler(req, client_clone.clone())
                }))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn relay_handler(
    mut req: Request<hyper::body::Incoming>,
    client: Client<HttpConnector, hyper::body::Incoming>,
) -> Result<Response<BoxedBody>> {

    // change dest here
    let destination_uri = "http://127.0.0.1:9000"; // Target server
    let path_query = req.uri().path_and_query().map(|x| x.as_str()).unwrap_or("");
    let new_uri = format!("{}{}", destination_uri, path_query);
    *req.uri_mut() = new_uri.parse()?;
    req.headers_mut().remove(hyper::header::HOST);
    req.headers_mut().remove(hyper::header::CONNECTION);

    match client.request(req).await {
        Ok(res) => {
            let (parts, body) = res.into_parts();
            let boxed_body = body.map_err(|e| e).boxed();
            Ok(Response::from_parts(parts, boxed_body))
        }
        Err(e) => {
            eprintln!("Relay error: {}", e);
            let mut res = Response::new(full("Internal Relay Error"));
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Ok(res)
        }
    }
}


fn full<T: Into<Bytes>>(chunk: T) -> BoxedBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}