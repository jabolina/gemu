#[tokio::main]
async fn main() {
    let transport = mepa::create_server(8080).await;
    assert!(transport.is_ok());

    let mut transport = transport.unwrap();
    let poll = transport.poll(|s| async move {
        println!("DATA: {}", s);
    });
    let await_polling = async move {
        poll.await;
        ()
    };

    tokio::join!(await_polling);
}
