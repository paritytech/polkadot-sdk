"use strict";
// Fix for createGame - should check if the returned game is actually new
async;
createGame(signer, PolkadotSigner, potAmount, bigint);
Promise < bigint | null > {
    try: {
        const: nextId = await this.api.query.Battleship.NextGameId.getValue({ at: "best" }),
        const: predictedId = typeof nextId === "bigint" ? nextId : BigInt(nextId),
        // Get current player game BEFORE creating
        const: pubkey = signer.publicKey,
        const: address = AccountId().dec(pubkey),
        const: oldPlayerGame = await this.api.query.Battleship.PlayerGame.getValue(address, { at: "best" }),
        const: oldGameId = oldPlayerGame ? (typeof oldPlayerGame === "bigint" ? oldPlayerGame : BigInt(oldPlayerGame)) : null,
        const: tx = this.api.tx.Battleship.create_game({ pot_amount: potAmount }),
        await, : .client, "create_game": , this: .api,
        // Poll for NEW game (different from old)
        for(let, i = 0, i, , , i) { }
    }++
};
{
    await new Promise(r => setTimeout(r, 1500));
    const playerGame = await this.api.query.Battleship.PlayerGame.getValue(address, { at: "best" });
    if (playerGame !== null && playerGame !== undefined) {
        const gameId = typeof playerGame === "bigint" ? playerGame : BigInt(playerGame);
        // Only return if it's a NEW game
        if (oldGameId === null || gameId !== oldGameId) {
            console.log(`[create_game] Confirmed NEW gameId=${gameId}`);
            return gameId;
        }
    }
}
// If we couldn't detect via PlayerGame, try using NextGameId
const newNextId = await this.api.query.Battleship.NextGameId.getValue({ at: "best" });
const newNext = typeof newNextId === "bigint" ? newNextId : BigInt(newNextId);
if (newNext > predictedId) {
    console.log(`[create_game] Inferred gameId=${predictedId} from NextGameId change`);
    return predictedId;
}
return null;
try { }
catch (e) {
    console.error("[create_game] Error:", e);
    return null;
}
