// Translate browser launch options into the same arguments the native parser
// consumes. Reject native-only operations before they reach filesystem/network
// code in Wasm. Unrelated query parameters (e.g. tracking tags) are ignored.
const choices = {
  renderer: ["text", "3d-walls"],
  motion: ["locomotion", "ease-out", "linear", "ease-in-out", "overshoot", "snap"],
  "portal-view": ["turn", "north"],
};
const maps = ["map1", "map1_test", "map2", "map3", "lightTest", "portals", "hunter_keys", "chasers"];
const nativeOnly = new Set(["screenshot", "record", "walk", "remote", "remote-port"]);

export function queryArguments(search) {
  const args = [];
  const seen = new Set();
  for (const [name, value] of new URLSearchParams(search)) {
    if (nativeOnly.has(name)) {
      throw new Error(`--${name} is only available in the native version`);
    }
    if (name !== "reveal" && name !== "map" && !Object.hasOwn(choices, name)) continue;
    if (seen.has(name)) throw new Error(`Duplicate URL option: ${name}`);
    seen.add(name);
    if (name === "reveal") {
      if (["", "true", "1"].includes(value)) args.push("--reveal");
      else if (!["false", "0"].includes(value)) throw new Error("reveal must be true, false, 1, or 0");
    } else if (name === "map") {
      const map = value.replace(/^assets\/maps\//, "").replace(/\.txt$/, "");
      if (!maps.includes(map)) throw new Error(`map must be one of: ${maps.join(", ")}`);
      args.push("--map", `assets/maps/${map}.txt`);
    } else {
      if (!choices[name].includes(value)) {
        throw new Error(`${name} must be one of: ${choices[name].join(", ")}`);
      }
      args.push(`--${name}`, value);
    }
  }
  return args;
}

// Called by Rust at startup, after wasm-bindgen has initialized the module.
export function startup_arguments() {
  return JSON.stringify(queryArguments(window.location.search));
}
