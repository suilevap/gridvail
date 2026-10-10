import assert from "node:assert/strict";
import { test } from "node:test";
import { queryArguments, startup_arguments } from "./options.mjs";

test("defaults, unrelated parameters and combined CLI options", () => {
  assert.deepEqual(queryArguments("?utm_source=link"), []);
  assert.deepEqual(queryArguments("?renderer=text&motion=ease%2Din%2Dout&portal-view=north&reveal"),
    ["--renderer", "text", "--motion", "ease-in-out", "--portal-view", "north", "--reveal"]);
});

test("map selection passes arbitrary names and decoded paths to Rust", () => {
  for (const map of ["new_map", "new_map.txt", "assets/maps/new_map.txt"]) {
    assert.deepEqual(queryArguments(`?map=${encodeURIComponent(map)}`), ["--map", map]);
  }
});

test("reveal supports bare flags and boolean values", () => {
  for (const value of ["", "true", "1"]) assert.deepEqual(queryArguments(`?reveal=${value}`), ["--reveal"]);
  for (const value of ["false", "0"]) assert.deepEqual(queryArguments(`?reveal=${value}`), []);
  assert.throws(() => queryArguments("?reveal=yes"), /reveal must be/);
});

test("empty, duplicate and native-only options fail", () => {
  for (const name of ["renderer", "motion", "map", "portal-view"]) {
    assert.throws(() => queryArguments(`?${name}`), /requires a value/);
    assert.throws(() => queryArguments(`?${name}=x&${name}=x`), /Duplicate/);
  }
  assert.throws(() => queryArguments("?reveal&reveal=false"), /Duplicate/);
  for (const name of ["screenshot", "record", "walk", "remote", "remote-port"]) {
    assert.throws(() => queryArguments(`?${name}`), /native version/);
  }
});

test("Wasm bridge returns arguments from the browser URL", () => {
  globalThis.window = { location: { search: "?motion=snap&reveal=1" } };
  try {
    assert.deepEqual(JSON.parse(startup_arguments()), ["--motion", "snap", "--reveal"]);
  } finally {
    delete globalThis.window;
  }
});
