'use strict';

// Run with Node.js and the Ajv 8.20.0 Draft 2020-12 entry point installed in a scratch directory.
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join, resolve } = require('node:path');

const entry = process.argv[2];
assert(entry, 'Usage: node tests/check-schemas.cjs <path-to-ajv/dist/2020.js>');
const Ajv2020 = require(resolve(entry)).default;
const ajv = new Ajv2020({ allErrors: true });
const schemaRoot = join(__dirname, '..', 'spec', '1.0');
const fixtures = join(__dirname, 'fixtures');
const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'));
const validators = new Map(['manifest', 'document', 'integrity'].map((name) => [
  name,
  ajv.compile(readJson(join(schemaRoot, name + '.schema.json'))),
]));

const corpus = readJson(join(fixtures, 'schema-conformance.json'));
for (const { name, schema, schema_accepts, input } of corpus.cases) {
  const validate = validators.get(schema);
  assert(validate, 'Unknown schema in ' + name);
  let value;
  switch (input.type) {
    case 'json':
      value = input.value;
      break;
    case 'raw':
      value = JSON.parse(input.value);
      break;
    case 'fixture':
      value = readJson(join(fixtures, input.value));
      break;
    default:
      throw new Error('Unknown fixture input type in ' + name);
  }
  assert.equal(validate(value), schema_accepts, `${name}: ${ajv.errorsText(validate.errors)}`);
}
console.log(`Draft 2020-12 conformance passed for ${corpus.cases.length} cases`);

