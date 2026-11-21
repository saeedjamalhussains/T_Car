#!/usr/bin/env node
'use strict';
/**
 * Regenerates T-Car's embedded datasets from the original (2014) npm packages:
 *   tel-carrier-db@1.1.3        -> datasets/nanp-prefixes.json.gz
 *                                  (NANP area code + exchange -> telco record)
 *   tel-carrier-gateways@2.0.0  -> datasets/nanp-carrier-gateways.json
 *                                  (carrier -> SMS/MMS gateway domains)
 *
 * datasets/calling-codes.json (ITU country codes) is hand-maintained.
 *
 * Usage:
 *   node scripts/build-datasets.js <tel-carrier-db dir> <tel-carrier-gateways dir>
 */
var fs = require('fs');
var path = require('path');
var zlib = require('zlib');

var dbDir = process.argv[2];
var gwDir = process.argv[3];
if (!dbDir || !gwDir) {
  console.error('usage: build-datasets.js <tel-carrier-db dir> <tel-carrier-gateways dir>');
  process.exit(1);
}

var out = path.join(__dirname, '..', 'datasets');
fs.mkdirSync(out, { recursive: true });

// ---- prefixes -------------------------------------------------------------
var data = JSON.parse(fs.readFileSync(path.join(dbDir, 'data.json'), 'utf8'));
var meta = JSON.parse(fs.readFileSync(path.join(dbDir, 'meta.json'), 'utf8'));

// Row layout in data.list[area]: [prefix, city, state, st, company, type, link, carrier, carrierName]
// A few meta tables contain nulls; coerce to "" so the consumer sees one type.
function strs(a) { return (a || []).map(function (v) { return v == null ? '' : String(v); }); }

var payload = {
  version: 1,
  source: 'tel-carrier-db@1.1.3 (localcallingguide/telcodata scrape)',
  meta: {
    cities: strs(meta.cities),
    states: strs(meta.states),
    sts: strs(meta.sts),
    companies: strs(meta.companies),
    types: strs(meta.types),
    links: strs(meta.links),
    carriers: strs(meta.carriers),
    carrierNames: strs(meta.carrierNames)
  },
  list: data.list
};
var json = JSON.stringify(payload);
fs.writeFileSync(path.join(out, 'nanp-prefixes.json.gz'), zlib.gzipSync(Buffer.from(json), { level: 9 }));
console.log('nanp-prefixes.json.gz', (fs.statSync(path.join(out, 'nanp-prefixes.json.gz')).size / 1024 / 1024).toFixed(2) + ' MB',
  'areas:', Object.keys(data.list).length);

// ---- carriers / gateways --------------------------------------------------
var wild = require(path.resolve(gwDir, 'carriers/in-the-wild.js'));
var historic = require(path.resolve(gwDir, 'carriers/historic.js'));

function conv(list, historicFlag) {
  return list.map(function (c) {
    return {
      name: c.name,
      carrier: c.carrier || c.name,
      wireless: (typeof c.wireless === 'boolean') ? c.wireless : null,
      sms: c.sms || null,
      mms: c.mms || null,
      // `test` was a JS regex predicate in the original package; we keep its
      // source so the Rust side can rebuild an equivalent matcher.
      test: c.test ? (String(c.test).match(/\/(.+)\/([a-z]*)/) || [])[1] || null : null,
      historic: historicFlag,
      updated: c.updated || null
    };
  });
}

var carriers = conv(wild, false).concat(conv(historic, true));
fs.writeFileSync(path.join(out, 'nanp-carrier-gateways.json'), JSON.stringify(carriers, null, 2));
console.log('nanp-carrier-gateways.json', carriers.length, 'carriers');
