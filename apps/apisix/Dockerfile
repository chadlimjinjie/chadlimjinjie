FROM apache/apisix:3.19.0-debian
ENV APISIX_STAND_ALONE=true
# The image ships an etcd config.yaml, so the standalone one has to replace it
COPY config.yaml apisix.yaml /usr/local/apisix/conf/
