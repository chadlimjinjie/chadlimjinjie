# apisix

APISIX gateway in standalone mode: routes live in `apisix.yaml` and are baked into the image. No etcd, no Admin API.

## Deploy (Railway / Render)

- Root directory: `apps/apisix`, build from the Dockerfile.
- Set `LOCATION_HOST` to the location-service host, without scheme (e.g. `my-project.vercel.app`). APISIX fails to start without it.
- `PORT` is injected by the platform and picked up in `config.yaml`.

To change routes, edit `apisix.yaml` and redeploy.

## Run locally

```sh
docker build -t apisix-gw apps/apisix
docker run --rm -p 9080:9080 -e LOCATION_HOST=my-project.vercel.app apisix-gw
curl -i http://localhost:9080/api/location
```
