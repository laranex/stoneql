// Test-owned loopback TCP relay for M006 fault injection (`npm run test:execution` only).
// Usage: node tools/tcp-fault-relay.mjs <target-port>. Prints one JSON line with the relay and
// control ports, then relays 127.0.0.1:<relay> → 127.0.0.1:<target> byte-for-byte (TLS passes
// end to end; nothing is decrypted or logged). Control: one command per line on the control port,
// answered with one line:
//   normal          new connections relay normally (existing connections keep their mode)
//   drop_s2c        current connections: keep client→server, discard server→client bytes
//   sever_on_reply  current connections: forward client→server, then destroy both sockets as
//                   soon as the server replies (the reply is never delivered)
//   sever           destroy every current connection now
//   refuse_new      destroy new client connections immediately
//   stats           JSON {open, accepted, refused}
import net from 'node:net';

const target = Number(process.argv[2]);
if (!Number.isInteger(target) || target <= 0 || target > 65535) throw Error('target port required');
const pairs = new Set();
let refuseNew = false;
let accepted = 0;
let refused = 0;

function destroy(pair) {
  pair.client.destroy();
  pair.server.destroy();
  pairs.delete(pair);
}
const relay = net.createServer(client => {
  if (refuseNew) {
    refused++;
    client.destroy();
    return;
  }
  accepted++;
  const server = net.connect({ host: '127.0.0.1', port: target });
  const pair = { client, server, mode: 'normal' };
  pairs.add(pair);
  client.on('data', chunk => server.write(chunk));
  server.on('data', chunk => {
    if (pair.mode === 'drop_s2c') return;
    if (pair.mode === 'sever_on_reply') return destroy(pair);
    client.write(chunk);
  });
  client.on('end', () => server.end());
  server.on('end', () => client.end());
  for (const socket of [client, server]) {
    socket.on('error', () => destroy(pair));
    socket.on('close', () => destroy(pair));
  }
});
const control = net.createServer(socket => {
  let buffer = '';
  socket.on('data', chunk => {
    buffer += chunk;
    let index;
    while ((index = buffer.indexOf('\n')) >= 0) {
      const command = buffer.slice(0, index).trim();
      buffer = buffer.slice(index + 1);
      let reply = 'ok';
      if (command === 'normal') refuseNew = false;
      else if (command === 'refuse_new') refuseNew = true;
      else if (command === 'drop_s2c' || command === 'sever_on_reply') for (const pair of pairs) pair.mode = command;
      else if (command === 'sever') for (const pair of [...pairs]) destroy(pair);
      else if (command === 'stats') reply = JSON.stringify({ open: pairs.size, accepted, refused });
      else reply = 'error';
      socket.write(`${reply}\n`);
    }
  });
  socket.on('error', () => socket.destroy());
});
relay.listen(0, '127.0.0.1', () => control.listen(0, '127.0.0.1', () => {
  console.log(JSON.stringify({ relayPort: relay.address().port, controlPort: control.address().port }));
}));
process.on('SIGTERM', () => process.exit(0));
