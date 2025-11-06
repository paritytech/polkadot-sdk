function parse_pjs_int(input) {
    return parseInt(input.replace(/,/g, ''));
}

async function run(nodeName, networkInfo) {
    const { wsUri, userDefinedTypes } = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    let blocks_per_para = {};

    await new Promise(async (resolve, _) => {
        let block_count = 0;
        let new_session_started = false;
        const unsubscribe = await api.query.system.events(async (events, block_hash) => {
            if (new_session_started) {
                block_count++;
            }

            events.forEach((record) => {
                const event = record.event;

                if (event.method == 'NewSession') {
                    new_session_started = true;
                    console.log(`New session started. Measuring CandidateIncluded events.`);
                }

                if (event.method != 'CandidateIncluded' || !new_session_started) {
                    return;
                }

                let included_para_id = parse_pjs_int(event.toHuman().data[0].descriptor.paraId);
                let relay_parent = event.toHuman().data[0].descriptor.relayParent;
                if (blocks_per_para[included_para_id] == undefined) {
                    blocks_per_para[included_para_id] = 1;
                } else {
                    blocks_per_para[included_para_id]++;
                }
                console.log(`CandidateIncluded for ${included_para_id}: block_offset=${block_count} relay_parent=${relay_parent}`);
            });

            if (block_count == 12) {
                unsubscribe();
                return resolve();
            }
        });
    });

    console.log(`Result: 2000: ${blocks_per_para[2000]}, 2001: ${blocks_per_para[2001]}`);
    // This check assumes that para 2000 runs slot based collator which respects its claim queue
    // and para 2001 runs lookahead which generates blocks for each relay parent.
    return (blocks_per_para[2000] >= 6) && (blocks_per_para[2001] <= 4);
}

module.exports = { run };
