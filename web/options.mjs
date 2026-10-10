// The schema is generated from the same Rust arms that parse native arguments.
export function queryArguments(search, schema) {
  const kinds = new Map(schema), args = [], seen = new Set();
  for (const [name, value] of new URLSearchParams(search)) {
    const option = `--${name}`, kind = kinds.get(option);
    if (!kind) continue;
    if (kind === "native") throw new Error(`${option} is only available in the native version`);
    if (seen.has(name)) throw new Error(`Duplicate URL option: ${name}`);
    seen.add(name);
    if (kind === "flag") {
      if (["", "true", "1"].includes(value)) args.push(option);
      else if (!["false", "0"].includes(value)) throw new Error(`${name} must be true, false, 1, or 0`);
    } else {
      if (!value) throw new Error(`${name} requires a value`);
      args.push(option, value);
    }
  }
  return args;
}

export function startup_arguments(schema) {
  return JSON.stringify(queryArguments(window.location.search, JSON.parse(schema)));
}
