mod cli;

use cli::node::Node;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let mut node = Node::init().await;
    node.run().await.unwrap();
}
