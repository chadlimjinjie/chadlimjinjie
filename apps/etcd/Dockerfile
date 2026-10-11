FROM quay.io/coreos/etcd:v3.5.21
# Local defaults; Railway overrides LISTEN/ADVERTISE with its own env vars
ENV ETCD_DATA_DIR=/etcd-data \
    ETCD_LISTEN_CLIENT_URLS=http://0.0.0.0:2379 \
    ETCD_ADVERTISE_CLIENT_URLS=http://etcd:2379
