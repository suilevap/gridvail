// Only translate URL syntax here; Rust owns CLI value validation and map lookup.
export function queryArguments(search) {
  const args = [], seen = new Set();
  for (const [name, value] of new URLSearchParams(search)) {
    if (["screenshot", "record", "walk", "remote", "remote-port"].includes(name)) {
      throw new Error(`--${name} is only available in the native version`);
    }
    if (!["renderer", "motion", "map", "reveal", "portal-view"].includes(name)) continue;
    if (seen.has(name)) throw new Error(`Duplicate URL option: ${name}`);
    seen.add(name);
    if (name === "reveal") {
      if (["", "true", "1"].includes(value)) args.push("--reveal");
      else if (!["false", "0"].includes(value)) throw new Error("reveal must be true, false, 1, or 0");
    } else {
      if (!value) throw new Error(`${name} requires a value`);
      args.push(`--${name}`, value);
    }
  }
  return args;
}

export function startup_arguments() {
  return JSON.stringify(queryArguments(window.location.search));
}
