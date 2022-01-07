use etcd_client as etcd;

pub(crate) struct EtcdReceiver {
    client: etcd::KvClient,
    
    partition: String,
}
