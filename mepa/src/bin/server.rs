#[tokio::main]
async fn main() {
    let (mut receiver, mut sender) = mepa::create(8080).await.expect("Failed creating transport");

    let messages = tokio::spawn(async move {
        loop {
            let mut message = String::new();
            println!("Write:");
            std::io::stdin()
                .read_line(&mut message)
                .expect("Could not read line!");

            if message.trim() == "quit" {
                break;
            }

            sender
                .send("localhost:8080", message.trim())
                .await
                .expect("Failed sending!");
        }
    });
    let poll = receiver.poll(|s| async move {
        println!("DATA: {}", s);
    });

    tokio::join!(poll, messages);
}
