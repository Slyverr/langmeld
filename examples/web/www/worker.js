// Runs the language server: messages in, replies out.
import init, { CalcServer } from "./pkg/calc_web.js";

const waiting = [];
let server;

const handle = (message) => {
  for (const reply of server.handle(JSON.stringify(message))) {
    postMessage(JSON.parse(reply));
  }
};

onmessage = (event) => (server ? handle(event.data) : waiting.push(event.data));

init().then(() => {
  server = new CalcServer();
  waiting.splice(0).forEach(handle);
});
