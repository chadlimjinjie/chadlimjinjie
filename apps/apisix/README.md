# apisix

APISIX gateway in traditional mode: routes live in etcd and are managed at runtime through the Admin API or the embedded dashboard at `:9180/ui/`. No redeploy to change routes.

## Run locally

etcd is its own app in `apps/etcd`. From the repo root:

```sh
docker network create apisix
docker build -t etcd apps/etcd
docker build -t apisix apps/apisix
# The name "etcd" is what config.yaml's default ETCD_HOST resolves
docker run -d --name etcd --network apisix -v etcd-data:/etcd-data etcd
# 9180 on loopback only, so the local admin key is never LAN-reachable
docker run --rm --network apisix -e ADMIN_KEY=dev-key \
  -p 9080:9080 -p 127.0.0.1:9180:9180 apisix
```

- Gateway: http://localhost:9080
- Dashboard: http://localhost:9180/ui/ (enter the admin key when prompted)

## Seed the location route

Routes are not in git. On a fresh etcd, create the route once:

```sh
# ADMIN_URL: http://localhost:9180 locally, the admin domain on Railway
curl -X PUT "$ADMIN_URL/apisix/admin/routes/location" -H "X-API-KEY: $ADMIN_KEY" -d '{
  "uri": "/api/location",
  "upstream": {
    "type": "roundrobin", "scheme": "https", "pass_host": "node",
    "nodes": [{ "host": "<location-service-host>", "port": 443, "weight": 1 }]
  }
}'
curl -i http://localhost:9080/api/location
```

`pass_host: node` matters: Vercel routes by Host header, so the upstream's own host must be sent, not the gateway's. The host is given without scheme (e.g. `my-project.vercel.app`).

## Seed the game routes

`apps/connect-four-service` (port 4110) and `apps/sudoku-service` (port 4111) are WebSocket servers. Same one-time seeding on a fresh etcd:

```sh
# hosts: host.docker.internal locally, <service>.railway.internal on Railway
curl -X PUT "$ADMIN_URL/apisix/admin/routes/connect_four_ws" -H "X-API-KEY: $ADMIN_KEY" -d '{
  "uri": "/connect-four/ws",
  "enable_websocket": true,
  "upstream": {
    "type": "roundrobin", "timeout": { "connect": 6, "send": 3600, "read": 3600 },
    "nodes": [{ "host": "<connect-four-host>", "port": 4110, "weight": 1 }]
  }
}'
curl -X PUT "$ADMIN_URL/apisix/admin/routes/sudoku_ws" -H "X-API-KEY: $ADMIN_KEY" -d '{
  "uri": "/sudoku/ws",
  "enable_websocket": true,
  "upstream": {
    "type": "roundrobin", "timeout": { "connect": 6, "send": 3600, "read": 3600 },
    "nodes": [{ "host": "<sudoku-host>", "port": 4111, "weight": 1 }]
  }
}'
```

`enable_websocket` is a route attribute, not a plugin, so nothing is added to `plugins:`. The hour-long timeouts stop APISIX from closing an idle game socket after its default 60 s. The web app reaches both through `VITE_API_URL`.

## Plugins

Only the plugins listed under `plugins:` in `config.yaml` are loaded, to keep CPU and memory low. To use another plugin, add it to that list and redeploy; a route that references an unlisted plugin breaks.

## Deploy (Railway)

Do these in order. Deploying the gateway before etcd is up, or before seeding, takes `/api/location` down.

1. **etcd service**: root directory `apps/etcd`, build from the Dockerfile, volume mounted at `/etcd-data`, **no public domain**. Env (overrides the Dockerfile's local defaults):
   - `ETCD_DATA_DIR=/etcd-data`
   - `ETCD_LISTEN_CLIENT_URLS=http://[::]:2379` (the private network may be IPv6-only)
   - `ETCD_ADVERTISE_CLIENT_URLS=http://etcd.railway.internal:2379`
2. **apisix service**: root directory `apps/apisix`, build from the Dockerfile. Env:
   - `ETCD_HOST=http://etcd.railway.internal:2379`
   - `ADMIN_KEY=<long random secret>` (APISIX fails to start without it)
   - `PORT=9080`
   - `LOCATION_HOST` is no longer used; remove it.
3. **Game services**: one service each for `apps/connect-four-service` and `apps/sudoku-service`, root directory set to that folder, build from the Dockerfile, **no public domain**. They must be in the same Railway project as the gateway, because `*.railway.internal` only resolves inside one project. Env: `PORT=4110` and `PORT=4111`.
4. **Domains**: gateway domain targets port `9080`; a second domain targets port `9180` for the Admin API and dashboard.
5. Run the seed commands against the admin domain, then open `https://<admin-domain>/ui/`.

## Security

The admin domain is internet-reachable and the admin key gives full control of the gateway. Keep it only in Railway secrets and rotate it if it leaks.

etcd is a single node on one volume: losing the volume loses every route.
