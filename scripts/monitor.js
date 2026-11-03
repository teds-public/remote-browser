'use strict';

const INTERVAL_MS = Number(process.env.MONITOR_INTERVAL_MS || 60_000);
const PORT = Number(process.env.PORT || 3000);
const URL = `http://localhost:${PORT}/healthz`;

async function ping() {
  const start = Date.now();
  try {
    const res = await fetch(URL);
    const body = await res.json();
    console.log(`${new Date().toISOString()} ok=${body.ok} tabs=${body.tabs} latency=${Date.now() - start}ms`);
  } catch (err) {
    console.log(`${new Date().toISOString()} FAIL ${err.message}`);
  }
}

setInterval(ping, INTERVAL_MS);
ping();
