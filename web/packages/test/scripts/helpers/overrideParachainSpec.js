const fs = require('fs');

function readSpec(filepath) {
    return JSON.parse(fs.readFileSync(filepath, 'utf8'));
}

function writeSpec(filepath, data) {
    return fs.writeFileSync(filepath, JSON.stringify(data, null, 4));
}

function replaceNested(data, keys, value) {
    if (keys.length === 0) {
        return value;
    }

    const key0 = isNaN(parseInt(keys[0])) ? keys[0] : parseInt(keys[0]);
    if (!(key0 in data)) {
        throw Error("Key '" + key + "' not found in original spec");
    }

    data[key0] = replaceNested(data[key0], keys.slice(1), value);
    return data;
}

function run() {
    const filepath = process.argv[2];
    if (!filepath) {
        throw Error("Expected filepath argument in first position");
    }

    const replacements = process.argv.slice(3);
    if (replacements.length % 2 !== 0) {
        throw Error("Expected an even number of key:value arguments in the form [(<key> <value>) ...]");
    }

    let specData = readSpec(filepath);
    for (i = 0; i < replacements.length; i += 2) {
        const key = replacements[i];
        let value = replacements[i + 1];
        try {
            value = JSON.parse(replacements[i + 1]);
        } catch {}
        specData = replaceNested(specData, key.split('.'), value);
    }
    writeSpec(filepath, specData);
}

run();
