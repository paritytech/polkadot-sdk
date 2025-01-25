function parse_pjs_int(input) {
    return parseInt(input.replace(/,/g, ''));
}

async function run(nodeName, networkInfo) {
    const { wsUri, userDefinedTypes } = networkInfo.nodesByName[nodeName];
    const api = await zombie.connect(wsUri, userDefinedTypes);

    let blocks_per_para = {};

    await new Promise(async (resolve, _) => {
        let block_count = 0;
        const unsubscribe = await api.query.system.events(async (events, block_hash) => {
            block_count++;

            events.forEach((record) => {
                const event = record.event;

                if (event.method != 'CandidateIncluded') {
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
    //
    // For 12 blocks there will be one session change. One block won't have anything backed/included.
    // In the next there will be one backed so for 12 blocks we should expect 10 included events - no
    // more than 4 for para 2001 and at least 6 for para 2000. This should also cover the unlucky
    // case when we observe two session changes during the 12 block period.
    return (blocks_per_para[2000] >= 6) && (blocks_per_para[2001] <= 4);
}

module.exports = { run };
