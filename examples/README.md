# 🎮 FaizDB Live Demonstration Applications

This directory contains two complete, production-ready sample web applications demonstrating how to build apps with FaizDB.

---

## 📁 1. Static HTML & JavaScript Web App (`examples/static-html-web/`)
A pure static web application with **zero backend server required**. It talks directly to FaizDB via HTTP REST API and listens to real-time database changes over WebSockets.

### How to Run:
1. Ensure FaizDB is running:
   ```bash
   faizdb serve --wire-port 27017 --http-port 27018
   ```
2. Simply double-click `examples/static-html-web/index.html` to open it in any web browser (Chrome, Edge, Safari, Firefox)!
3. Add a player score, watch it save to FaizDB, and see live WebSocket change events flash in real time!

---

## 📁 2. Fullstack Node.js & Express Web App (`examples/nodejs-express-web/`)
A modern SaaS dashboard built with Node.js and Express that connects to FaizDB using the **official MongoDB Node.js driver** (`mongodb`) over port 27017.

### How to Run:
1. Ensure FaizDB is running:
   ```bash
   faizdb serve --wire-port 27017 --http-port 27018
   ```
2. Open terminal in `examples/nodejs-express-web/`:
   ```bash
   cd examples/nodejs-express-web
   pnpm install  # or npm install
   pnpm start    # or npm start
   ```
3. Open `http://localhost:3000` in your browser.
4. Add enterprise customers and watch the **Aggregation Analytics Pipeline** calculate revenue by subscription tier in sub-milliseconds!
