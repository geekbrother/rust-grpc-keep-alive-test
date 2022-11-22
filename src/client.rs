pub mod pb {
  tonic::include_proto!("grpc.examples.echo");
}

use futures::stream::Stream;
use std::time::Duration;
use tokio_stream::StreamExt;
use tonic::transport::Channel;

use pb::{echo_client::EchoClient, EchoRequest};

fn echo_requests_iter() -> impl Stream<Item = EchoRequest> {
  tokio_stream::iter(1..usize::MAX).map(|i| EchoRequest {
      message: format!("msg {:02}", i),
  })
}

async fn bidirectional_streaming_echo_throttle(client: &mut EchoClient<Channel>, dur: Duration) {
  let in_stream = echo_requests_iter().throttle(dur);

  let response = client
      .bidirectional_streaming_echo(in_stream)
      .await
      .unwrap();

  let mut resp_stream = response.into_inner();

  while let Some(received) = resp_stream.next().await {
      let received = received.unwrap();
      println!("\treceived message: `{}`", received.message);
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let endpoint = tonic::transport::Endpoint::from_static("http://[::1]:50051")
      .timeout(Duration::from_secs(5))
      .tcp_keepalive(Some(Duration::from_secs(3)));

  let mut client = EchoClient::connect(endpoint).await.unwrap();

  // Echo stream that sends up to `usize::MAX` requets. One request each 2s.
  // Exiting client with CTRL+C demonstrate how to distinguish broken pipe from
  //graceful client disconnection (above example) on the server side.
  println!("\r\nBidirectional stream echo (kill client with CTLR+C):");
  bidirectional_streaming_echo_throttle(&mut client, Duration::from_secs(600)).await;

  Ok(())
}
