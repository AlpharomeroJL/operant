/**
 * Resolves with the next `eventName` payload emitted by `emitter`.
 * @param {import("node:events").EventEmitter} emitter
 * @param {string} eventName
 * @returns {Promise<any>}
 */
export function waitForEvent(emitter, eventName) {
  return new Promise((resolve) => {
    emitter.once(eventName, resolve);
  });
}
