import assert from "node:assert/strict";
import { test } from "node:test";
import { queryArguments, startup_arguments } from "./options.mjs";

// These synthetic options demonstrate that JavaScript knows no game option names.
const schema = [["--setting", "value"], ["--enabled", "flag"], ["--native-tool", "native"]];

test("schema drives value options, flags and unrelated parameter filtering", () => {
  assert.deepEqual(queryArguments("?utm_source=link", schema), []);
  assert.deepEqual(queryArguments("?setting=assets%2Fmaps%2Fnew.txt&enabled", schema),
    ["--setting", "assets/maps/new.txt", "--enabled"]);
  for (const value of ["", "true", "1"]) assert.deepEqual(queryArguments(`?enabled=${value}`, schema), ["--enabled"]);
  for (const value of ["false", "0"]) assert.deepEqual(queryArguments(`?enabled=${value}`, schema), []);
});

test("new options only need to appear in the supplied Rust schema", () => {
  const extended = [...schema, ["--future-argument", "value"], ["--future-flag", "flag"]];
  assert.deepEqual(queryArguments("?future-argument=1&future-flag=true", extended),
    ["--future-argument", "1", "--future-flag"]);
});

test("invalid flags, empty values, duplicates and native-only options fail", () => {
  for (const query of ["enabled=yes", "setting", "setting=x&setting=x", "enabled&enabled=false", "native-tool"]) {
    assert.throws(() => queryArguments(`?${query}`, schema));
  }
});

test("Wasm bridge receives its schema from Rust and reads the browser URL", () => {
  globalThis.window = { location: { search: "?setting=x&enabled=1" } };
  try {
    assert.deepEqual(JSON.parse(startup_arguments(JSON.stringify(schema))), ["--setting", "x", "--enabled"]);
  } finally {
    delete globalThis.window;
  }
});
