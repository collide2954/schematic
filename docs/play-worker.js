let exports = null;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

const ready = fetch("schematic.wasm")
  .then((response) => response.arrayBuffer())
  .then((bytes) => WebAssembly.instantiate(bytes, {}))
  .then((result) => {
    exports = result.instance.exports;
  });

function evaluate(source) {
  const bytes = encoder.encode(source);
  const pointer = exports.alloc(bytes.length);
  new Uint8Array(exports.memory.buffer, pointer, bytes.length).set(bytes);
  const length = exports.eval(pointer, bytes.length);
  const start = exports.result_ptr();
  return decoder.decode(new Uint8Array(exports.memory.buffer, start, length));
}

self.onmessage = async (event) => {
  const { id, action, source } = event.data;
  try {
    await ready;
    if (action === "reset") {
      exports.reset();
      postMessage({ id, text: "" });
      return;
    }
    postMessage({ id, text: evaluate(source) });
  } catch (error) {
    postMessage({ id, text: "The interpreter stopped: " + error.message + "\n", crashed: true });
  }
};
