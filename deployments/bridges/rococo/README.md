# Rococo deployments

This folder contains some information and useful stuff from our other test deployment on Rococo.

## Grafana Alerts and Dashboards

JSON model for Grafana alerts and dashobards that we use, may be found in the [dasboard/grafana](./dashboard/grafana/)
folder.

**Dashboards:**
- rococo-beefy-dashboard.json (exported JSON directly from https://grafana.teleport.parity.io/dashboards/f/eblDiw17z/Bridges)

**Alerts:**
- rococo-beefy-alerts.json https://grafana.teleport.parity.io/api/ruler/grafana/api/v1/rules/Bridges/Rococo%20BEEFY

_Note: All json files are formatted with `jq . file.json`._