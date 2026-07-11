// Patches the lowest-level socket constructors Node exposes (net, dgram,
// tls) so any attempt to open a network socket during the guarded window
// throws immediately instead of silently phoning home. This is the mechanism
// behind docs/specs/voice.md's "a CI test asserts the voice sidecar opens
// zero network sockets": it proves this sidecar's own code path never
// reaches for net/dgram/tls, rather than trusting that it simply "did not
// happen to" in one run.
//
// Deliberately not under test/: node --test treats every file inside any
// directory literally named "test" as a candidate test file, regardless of
// its name, so a shared helper has to live outside that tree.

import net from "node:net";
import dgram from "node:dgram";
import tls from "node:tls";

export function installNetworkGuard() {
  const calls = [];
  const originals = {
    connect: net.connect,
    createConnection: net.createConnection,
    socketConnect: net.Socket.prototype.connect,
    dgramCreateSocket: dgram.createSocket,
    tlsConnect: tls.connect,
  };

  function trip(kind) {
    calls.push(kind);
    throw new Error(`network guard: blocked attempt to open a ${kind} socket`);
  }

  net.connect = () => trip("net.connect");
  net.createConnection = () => trip("net.createConnection");
  net.Socket.prototype.connect = function blockedConnect() {
    trip("net.Socket#connect");
  };
  dgram.createSocket = () => trip("dgram.createSocket");
  tls.connect = () => trip("tls.connect");

  return {
    calls,
    count: () => calls.length,
    restore() {
      net.connect = originals.connect;
      net.createConnection = originals.createConnection;
      net.Socket.prototype.connect = originals.socketConnect;
      dgram.createSocket = originals.dgramCreateSocket;
      tls.connect = originals.tlsConnect;
    },
  };
}
