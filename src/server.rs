pub mod pb {
  tonic::include_proto!("grpc.examples.echo");
}

use futures::Stream;
use std::{error::Error, io::ErrorKind, net::ToSocketAddrs, pin::Pin, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status, Streaming};

use pb::{EchoRequest, EchoResponse};

type EchoResult<T> = Result<Response<T>, Status>;
type ResponseStream = Pin<Box<dyn Stream<Item = Result<EchoResponse, Status>> + Send>>;

fn match_for_io_error(err_status: &Status) -> Option<&std::io::Error> {
  let mut err: &(dyn Error + 'static) = err_status;

  loop {
      if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
          return Some(io_err);
      }

      // h2::Error do not expose std::io::Error with `source()`
      // https://github.com/hyperium/h2/pull/462
      if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
          if let Some(io_err) = h2_err.get_io() {
              return Some(io_err);
          }
      }

      err = match err.source() {
          Some(err) => err,
          None => return None,
      };
  }
}

#[derive(Debug)]
pub struct EchoServer {}

#[tonic::async_trait]
impl pb::echo_server::Echo for EchoServer {
  type BidirectionalStreamingEchoStream = ResponseStream;
  async fn bidirectional_streaming_echo(
      &self,
      req: Request<Streaming<EchoRequest>>,
  ) -> EchoResult<Self::BidirectionalStreamingEchoStream> {
      println!("EchoServer::bidirectional_streaming_echo");

      let mut in_stream = req.into_inner();
      let (tx, rx) = mpsc::channel(128);

      // this spawn here is required if you want to handle connection error.
      // If we just map `in_stream` and write it back as `out_stream` the `out_stream`
      // will be drooped when connection error occurs and error will never be propagated
      // to mapped version of `in_stream`.
      tokio::spawn(async move {
          while let Some(result) = in_stream.next().await {
              match result {
                  Ok(v) => tx
                      .send(Ok(EchoResponse { message: v.message }))
                      .await
                      .expect("working rx"),
                  Err(err) => {
                      if let Some(io_err) = match_for_io_error(&err) {
                          if io_err.kind() == ErrorKind::BrokenPipe {
                              // here you can handle special case when client
                              // disconnected in unexpected way
                              eprintln!("\tclient disconnected: broken pipe");
                              break;
                          }
                      }

                      match tx.send(Err(err)).await {
                          Ok(_) => (),
                          Err(_err) => break, // response was droped
                      }
                  }
              }
          }
          println!("\tstream ended");
      });

      // echo just write the same data that was received
      let out_stream = ReceiverStream::new(rx);

      Ok(Response::new(
          Box::pin(out_stream) as Self::BidirectionalStreamingEchoStream
      ))
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let server = EchoServer {};
  Server::builder()
      .http2_keepalive_interval(Some(Duration::from_secs(3)))
      .http2_keepalive_timeout(Some(Duration::from_secs(3)))
      .max_concurrent_streams(Some(128))
      .add_service(pb::echo_server::EchoServer::new(server))
      .serve("[::]:50051".to_socket_addrs().unwrap().next().unwrap())
      .await
      .unwrap();

  Ok(())
}
