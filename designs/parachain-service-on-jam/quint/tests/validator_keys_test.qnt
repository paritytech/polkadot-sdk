// Chunked `designate` flow (design §5.3).
module validator_keys_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import validator_keys.* from "../validator_keys"
  import accumulate.* from "../accumulate"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  pure def vkeys(n: int): List[ValidatorKey] =
    range(0, n).foldl(List(), (l, i) => l.append(i))

  /// A partial chunk is buffered and `designate` is not called.
  run partialChunkBuffersTest = {
    val o = applySetValidatorKeys(initialState, vkeys(30), false)
    all {
      assert(o.state.stagedValidatorKeys.length() == 30),
      assert(not(o.jamDesignate)),
      init
    }
  }

  /// §5.3 abort path: `SetValidatorKeys { keys: [], is_last: true }` discards the
  /// staging area without calling `designate`, whatever is buffered. Treating it
  /// as "submit the buffer" would install a truncated set whenever the buffered
  /// length is a valid `valcount`, which a 30-key chunk already is.
  run emptyFinalChunkAbortsTest = {
    val fromEmpty = applySetValidatorKeys(initialState, List(), true)
    val staged = applySetValidatorKeys(initialState, vkeys(30), false).state
    val fromStaged = applySetValidatorKeys(staged, List(), true)
    all {
      assert(not(fromEmpty.jamDesignate)),
      assert(fromEmpty.state.stagedValidatorKeys == List()),
      // 30 = 3 * 10 is a valid `valcount`, so this is exactly the case that
      // must NOT reach `designate`.
      assert(ValCount.contains(30)),
      assert(staged.stagedValidatorKeys.length() == 30),
      assert(not(fromStaged.jamDesignate)),
      assert(fromStaged.designateVec == List()),
      assert(fromStaged.state.stagedValidatorKeys == List()),
      init
    }
  }

  /// A non-empty final chunk still submits `buffer ++ keys`, and a length
  /// outside `valcount` is rejected with the buffer cleared either way.
  run finalChunkSubmitsAssembledSetTest = {
    val staged = applySetValidatorKeys(initialState, vkeys(30), false).state
    // 30 + 30 = 60 = 3 * 20, a valid `valcount`.
    val ok = applySetValidatorKeys(staged, vkeys(30), true)
    // 30 + 1 = 31, not a multiple of 3.
    val bad = applySetValidatorKeys(staged, vkeys(1), true)
    all {
      assert(ok.jamDesignate),
      assert(ok.designateVec.length() == 60),
      assert(ok.state.stagedValidatorKeys == List()),
      assert(not(bad.jamDesignate)),
      assert(bad.log == Some(DesignateRejected({ len: 31 }))),
      assert(bad.state.stagedValidatorKeys == List()),
      init
    }
  }
}
