/**
 * Pure-JS BLAKE3 (vendored from blake3-optimized, Apache-2.0).
 * Hub uses this for content-addressed object ids with zero runtime deps.
 */

const blake3Impl = {};

/**
 * BLAKE3 - Ultra-Optimized Pure JavaScript Implementation with WASM SIMD
 *
 * Optimizations:
 * 1. Fully unrolled compress function (no loops in hot path)
 * 2. All state in local SMI variables
 * 3. Direct message word permutation (no array copies)
 * 4. Pre-allocated reusable buffers
 * 5. Direct Uint32Array view for aligned LE input
 * 6. WASM SIMD compress4x for 4-way parallel chunk processing
 *
 * @author Implementation for Zooko's bounty
 * @see https://x.com/zooko/status/1998185559542657145
 */

(function(exports) {
  'use strict';

  // WASM SIMD compressChunks4x module (base64 encoded) - processes all 16 blocks in one call
  const WASM_SIMD_B64 = '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';

  const IV = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);

  const BLOCK_LEN = 64;
  const CHUNK_LEN = 1024;
  const CHUNK_START = 1;
  const CHUNK_END = 2;
  const PARENT = 4;
  const ROOT = 8;
  const KEYED_HASH = 16;
  const DERIVE_KEY_CONTEXT = 32;
  const DERIVE_KEY_MATERIAL = 64;

  const blockWords = new Uint32Array(16);
  let cvStack = null;

  // WASM SIMD state
  let wasmSimdEnabled = false;
  let wasmCompress4x = null;
  let wasmMem32 = null;

  function getCvStack(size) {
    if (cvStack === null || cvStack.length < size) {
      cvStack = new Uint32Array(size);
    }
    return cvStack;
  }

  // Ultra-optimized compress: fully unrolled, all local variables
  function compress(cv, cvOffset, m, mOffset, out, outOffset, truncateOutput, counter, blockLen, flags) {
    let m_0 = m[mOffset + 0] | 0;
    let m_1 = m[mOffset + 1] | 0;
    let m_2 = m[mOffset + 2] | 0;
    let m_3 = m[mOffset + 3] | 0;
    let m_4 = m[mOffset + 4] | 0;
    let m_5 = m[mOffset + 5] | 0;
    let m_6 = m[mOffset + 6] | 0;
    let m_7 = m[mOffset + 7] | 0;
    let m_8 = m[mOffset + 8] | 0;
    let m_9 = m[mOffset + 9] | 0;
    let m_10 = m[mOffset + 10] | 0;
    let m_11 = m[mOffset + 11] | 0;
    let m_12 = m[mOffset + 12] | 0;
    let m_13 = m[mOffset + 13] | 0;
    let m_14 = m[mOffset + 14] | 0;
    let m_15 = m[mOffset + 15] | 0;

    let s_0 = cv[cvOffset + 0] | 0;
    let s_1 = cv[cvOffset + 1] | 0;
    let s_2 = cv[cvOffset + 2] | 0;
    let s_3 = cv[cvOffset + 3] | 0;
    let s_4 = cv[cvOffset + 4] | 0;
    let s_5 = cv[cvOffset + 5] | 0;
    let s_6 = cv[cvOffset + 6] | 0;
    let s_7 = cv[cvOffset + 7] | 0;
    let s_8 = 0x6a09e667 | 0;
    let s_9 = 0xbb67ae85 | 0;
    let s_10 = 0x3c6ef372 | 0;
    let s_11 = 0xa54ff53a | 0;
    let s_12 = counter | 0;
    let s_13 = (counter / 0x100000000) | 0;
    let s_14 = blockLen | 0;
    let s_15 = flags | 0;

    for (let r = 0; r < 7; r++) {
      s_0 = (((s_0 + s_4) | 0) + m_0) | 0; s_12 ^= s_0; s_12 = (s_12 >>> 16) | (s_12 << 16);
      s_8 = (s_8 + s_12) | 0; s_4 ^= s_8; s_4 = (s_4 >>> 12) | (s_4 << 20);
      s_0 = (((s_0 + s_4) | 0) + m_1) | 0; s_12 ^= s_0; s_12 = (s_12 >>> 8) | (s_12 << 24);
      s_8 = (s_8 + s_12) | 0; s_4 ^= s_8; s_4 = (s_4 >>> 7) | (s_4 << 25);

      s_1 = (((s_1 + s_5) | 0) + m_2) | 0; s_13 ^= s_1; s_13 = (s_13 >>> 16) | (s_13 << 16);
      s_9 = (s_9 + s_13) | 0; s_5 ^= s_9; s_5 = (s_5 >>> 12) | (s_5 << 20);
      s_1 = (((s_1 + s_5) | 0) + m_3) | 0; s_13 ^= s_1; s_13 = (s_13 >>> 8) | (s_13 << 24);
      s_9 = (s_9 + s_13) | 0; s_5 ^= s_9; s_5 = (s_5 >>> 7) | (s_5 << 25);

      s_2 = (((s_2 + s_6) | 0) + m_4) | 0; s_14 ^= s_2; s_14 = (s_14 >>> 16) | (s_14 << 16);
      s_10 = (s_10 + s_14) | 0; s_6 ^= s_10; s_6 = (s_6 >>> 12) | (s_6 << 20);
      s_2 = (((s_2 + s_6) | 0) + m_5) | 0; s_14 ^= s_2; s_14 = (s_14 >>> 8) | (s_14 << 24);
      s_10 = (s_10 + s_14) | 0; s_6 ^= s_10; s_6 = (s_6 >>> 7) | (s_6 << 25);

      s_3 = (((s_3 + s_7) | 0) + m_6) | 0; s_15 ^= s_3; s_15 = (s_15 >>> 16) | (s_15 << 16);
      s_11 = (s_11 + s_15) | 0; s_7 ^= s_11; s_7 = (s_7 >>> 12) | (s_7 << 20);
      s_3 = (((s_3 + s_7) | 0) + m_7) | 0; s_15 ^= s_3; s_15 = (s_15 >>> 8) | (s_15 << 24);
      s_11 = (s_11 + s_15) | 0; s_7 ^= s_11; s_7 = (s_7 >>> 7) | (s_7 << 25);

      s_0 = (((s_0 + s_5) | 0) + m_8) | 0; s_15 ^= s_0; s_15 = (s_15 >>> 16) | (s_15 << 16);
      s_10 = (s_10 + s_15) | 0; s_5 ^= s_10; s_5 = (s_5 >>> 12) | (s_5 << 20);
      s_0 = (((s_0 + s_5) | 0) + m_9) | 0; s_15 ^= s_0; s_15 = (s_15 >>> 8) | (s_15 << 24);
      s_10 = (s_10 + s_15) | 0; s_5 ^= s_10; s_5 = (s_5 >>> 7) | (s_5 << 25);

      s_1 = (((s_1 + s_6) | 0) + m_10) | 0; s_12 ^= s_1; s_12 = (s_12 >>> 16) | (s_12 << 16);
      s_11 = (s_11 + s_12) | 0; s_6 ^= s_11; s_6 = (s_6 >>> 12) | (s_6 << 20);
      s_1 = (((s_1 + s_6) | 0) + m_11) | 0; s_12 ^= s_1; s_12 = (s_12 >>> 8) | (s_12 << 24);
      s_11 = (s_11 + s_12) | 0; s_6 ^= s_11; s_6 = (s_6 >>> 7) | (s_6 << 25);

      s_2 = (((s_2 + s_7) | 0) + m_12) | 0; s_13 ^= s_2; s_13 = (s_13 >>> 16) | (s_13 << 16);
      s_8 = (s_8 + s_13) | 0; s_7 ^= s_8; s_7 = (s_7 >>> 12) | (s_7 << 20);
      s_2 = (((s_2 + s_7) | 0) + m_13) | 0; s_13 ^= s_2; s_13 = (s_13 >>> 8) | (s_13 << 24);
      s_8 = (s_8 + s_13) | 0; s_7 ^= s_8; s_7 = (s_7 >>> 7) | (s_7 << 25);

      s_3 = (((s_3 + s_4) | 0) + m_14) | 0; s_14 ^= s_3; s_14 = (s_14 >>> 16) | (s_14 << 16);
      s_9 = (s_9 + s_14) | 0; s_4 ^= s_9; s_4 = (s_4 >>> 12) | (s_4 << 20);
      s_3 = (((s_3 + s_4) | 0) + m_15) | 0; s_14 ^= s_3; s_14 = (s_14 >>> 8) | (s_14 << 24);
      s_9 = (s_9 + s_14) | 0; s_4 ^= s_9; s_4 = (s_4 >>> 7) | (s_4 << 25);

      const t0 = m_0, t1 = m_1, t2 = m_2, t3 = m_3, t4 = m_4, t5 = m_5, t6 = m_6, t7 = m_7;
      const t8 = m_8, t9 = m_9, t10 = m_10, t11 = m_11, t12 = m_12, t13 = m_13, t14 = m_14, t15 = m_15;
      m_0 = t2; m_1 = t6; m_2 = t3; m_3 = t10; m_4 = t7; m_5 = t0; m_6 = t4; m_7 = t13;
      m_8 = t1; m_9 = t11; m_10 = t12; m_11 = t5; m_12 = t9; m_13 = t14; m_14 = t15; m_15 = t8;
    }

    out[outOffset + 0] = s_0 ^ s_8;
    out[outOffset + 1] = s_1 ^ s_9;
    out[outOffset + 2] = s_2 ^ s_10;
    out[outOffset + 3] = s_3 ^ s_11;
    out[outOffset + 4] = s_4 ^ s_12;
    out[outOffset + 5] = s_5 ^ s_13;
    out[outOffset + 6] = s_6 ^ s_14;
    out[outOffset + 7] = s_7 ^ s_15;
    if (!truncateOutput) {
      out[outOffset + 8] = s_8 ^ cv[cvOffset + 0];
      out[outOffset + 9] = s_9 ^ cv[cvOffset + 1];
      out[outOffset + 10] = s_10 ^ cv[cvOffset + 2];
      out[outOffset + 11] = s_11 ^ cv[cvOffset + 3];
      out[outOffset + 12] = s_12 ^ cv[cvOffset + 4];
      out[outOffset + 13] = s_13 ^ cv[cvOffset + 5];
      out[outOffset + 14] = s_14 ^ cv[cvOffset + 6];
      out[outOffset + 15] = s_15 ^ cv[cvOffset + 7];
    }
  }

  function wordsToBytes(words) {
    const bytes = new Uint8Array(words.length * 4);
    const view = new DataView(bytes.buffer);
    for (let i = 0; i < words.length; i++) {
      view.setUint32(i * 4, words[i], true);
    }
    return bytes;
  }

  // Pre-allocated CV arrays for SIMD output
  const simdCvs = [new Uint32Array(8), new Uint32Array(8), new Uint32Array(8), new Uint32Array(8)];

  // Process 4 complete chunks in parallel using SIMD - SINGLE WASM CALL
  // Memory layout for compressChunks4x:
  //   Offset 0-4095 (bytes): 16 blocks × 16 words × 4 chunks = 1024 v128s, word-major
  //   Offset 4096-4111: Counter v128 (4 chunk counters)
  //   Offset 4112-4239: Output CVs (8 × v128 = 128 bytes)
  function processChunks4xSimd(input, inputOffset, baseChunkCounter) {
    // Pre-transpose input to word-major layout:
    // For each block b (0-15), word w (0-15):
    //   mem32[b*64 + w*4 + c] = input[chunk_c][block_b][word_w]

    const inputAligned = (input.byteOffset + inputOffset) % 4 === 0;

    if (inputAligned) {
      // Fast path: aligned input, use Uint32Array directly
      const inputView = new Uint32Array(input.buffer, input.byteOffset + inputOffset);
      // Chunk offsets in words (each chunk = 1024 bytes = 256 words)
      const c0 = 0, c1 = 256, c2 = 512, c3 = 768;

      for (let block = 0; block < 16; block++) {
        const blockWordOff = block * 16;  // offset within chunk in words
        const dstBlockOff = block * 64;   // destination offset (block * 16 words * 4 chunks)

        for (let w = 0; w < 16; w++) {
          const dst = dstBlockOff + w * 4;
          wasmMem32[dst]     = inputView[c0 + blockWordOff + w];
          wasmMem32[dst + 1] = inputView[c1 + blockWordOff + w];
          wasmMem32[dst + 2] = inputView[c2 + blockWordOff + w];
          wasmMem32[dst + 3] = inputView[c3 + blockWordOff + w];
        }
      }
    } else {
      // Slow path: unaligned input, byte-by-byte
      for (let block = 0; block < 16; block++) {
        const dstBlockOff = block * 64;
        for (let w = 0; w < 16; w++) {
          const dst = dstBlockOff + w * 4;
          for (let c = 0; c < 4; c++) {
            const byteOff = c * CHUNK_LEN + block * BLOCK_LEN + w * 4;
            wasmMem32[dst + c] =
              input[inputOffset + byteOff] |
              (input[inputOffset + byteOff + 1] << 8) |
              (input[inputOffset + byteOff + 2] << 16) |
              (input[inputOffset + byteOff + 3] << 24);
          }
        }
      }
    }

    // Write counters at offset 4096 (word offset 1024)
    wasmMem32[1024] = baseChunkCounter;
    wasmMem32[1025] = baseChunkCounter + 1;
    wasmMem32[1026] = baseChunkCounter + 2;
    wasmMem32[1027] = baseChunkCounter + 3;

    // Single WASM call processes all 16 blocks with CV state in locals
    wasmCompress4x();

    // Read output CVs from offset 4112 (word offset 1028)
    // Output is 8 × v128, word-major: [cv0_chunk0..3, cv1_chunk0..3, ...]
    for (let w = 0; w < 8; w++) {
      const src = 1028 + w * 4;
      simdCvs[0][w] = wasmMem32[src];
      simdCvs[1][w] = wasmMem32[src + 1];
      simdCvs[2][w] = wasmMem32[src + 2];
      simdCvs[3][w] = wasmMem32[src + 3];
    }
    return simdCvs;
  }

  function hash(input, outputLen) {
    if (typeof input === 'string') {
      input = new TextEncoder().encode(input);
    }
    if (!(input instanceof Uint8Array)) {
      input = new Uint8Array(input);
    }

    outputLen = outputLen || 32;

    // For extended output (XOF), use the Hasher class which supports it
    if (outputLen > 32) {
      const hasher = new Hasher();
      hasher.update(input);
      return hasher.finalize(outputLen);
    }

    const out = new Uint32Array(8);
    const totalLen = input.length;
    const flags = 0;

    // Direct Uint32Array view for aligned LE input
    let inputWords = null;
    if (input.byteOffset % 4 === 0) {
      inputWords = new Uint32Array(input.buffer, input.byteOffset, input.length >> 2);
    }

    if (totalLen === 0) {
      blockWords.fill(0);
      compress(IV, 0, blockWords, 0, out, 0, true, 0, 0, CHUNK_START | CHUNK_END | ROOT);
      return wordsToBytes(out).slice(0, outputLen);
    }

    const numChunks = Math.ceil(totalLen / CHUNK_LEN);
    const maxDepth = Math.ceil(Math.log2(numChunks + 1)) + 2;
    const stack = getCvStack(maxDepth * 8);
    let stackPos = 0;
    let offset = 0;
    let chunkCounter = 0;

    // SIMD path: process 4 full chunks at a time
    // Uses WASM SIMD to process 4 independent chunks in parallel
    if (wasmSimdEnabled && totalLen >= 4 * CHUNK_LEN) {
      while (offset + 4 * CHUNK_LEN <= totalLen) {
        // Process 4 chunks with SIMD
        const cvs = processChunks4xSimd(input, offset, chunkCounter);

        // Add all 4 CVs to stack and merge
        for (let i = 0; i < 4; i++) {
          stack.set(cvs[i], stackPos);
          stackPos += 8;
          chunkCounter++;

          // Check if this is the last chunk of input
          const isLastChunk = (offset + 4 * CHUNK_LEN >= totalLen) && (i === 3);

          // Merge pairs in Merkle tree, but stop before final merge if this is the last chunk
          let tc = chunkCounter;
          while ((tc & 1) === 0 && stackPos > 8) {
            // Don't do the final merge here - leave it for finalization with ROOT flag
            if (isLastChunk && stackPos === 16) break;
            stackPos -= 16;
            compress(IV, 0, stack, stackPos, stack, stackPos, true, 0, BLOCK_LEN, flags | PARENT);
            stackPos += 8;
            tc >>= 1;
          }
        }
        offset += 4 * CHUNK_LEN;
      }
    }

    // Scalar path for remaining chunks
    while (offset < totalLen) {
      const chunkStart = offset;
      const chunkEnd = Math.min(offset + CHUNK_LEN, totalLen);
      const chunkLen = chunkEnd - chunkStart;
      const isLastChunk = chunkEnd === totalLen;

      stack.set(IV, stackPos);
      const numBlocks = Math.ceil(chunkLen / BLOCK_LEN);

      for (let block = 0; block < numBlocks; block++) {
        const blockStart = chunkStart + block * BLOCK_LEN;
        const blockEnd = Math.min(blockStart + BLOCK_LEN, chunkEnd);
        const blockLen = blockEnd - blockStart;
        const isFirstBlock = block === 0;
        const isLastBlockOfChunk = block === numBlocks - 1;

        let blockFlags = flags;
        if (isFirstBlock) blockFlags |= CHUNK_START;
        if (isLastBlockOfChunk) blockFlags |= CHUNK_END;
        if (isLastBlockOfChunk && isLastChunk && chunkCounter === 0) {
          blockFlags |= ROOT;
        }

        if (blockLen === BLOCK_LEN && inputWords && blockStart % 4 === 0) {
          compress(stack, stackPos, inputWords, blockStart >> 2, stack, stackPos, true, chunkCounter, BLOCK_LEN, blockFlags);
        } else {
          blockWords.fill(0);
          for (let i = 0; i < blockLen; i++) {
            blockWords[i >> 2] |= input[blockStart + i] << ((i & 3) * 8);
          }
          compress(stack, stackPos, blockWords, 0, stack, stackPos, true, chunkCounter, blockLen, blockFlags);
        }
      }

      stackPos += 8;
      chunkCounter++;
      offset = chunkEnd;

      if (!isLastChunk) {
        let totalChunks = chunkCounter;
        while ((totalChunks & 1) === 0) {
          stackPos -= 16;
          compress(IV, 0, stack, stackPos, stack, stackPos, true, 0, BLOCK_LEN, flags | PARENT);
          stackPos += 8;
          totalChunks >>= 1;
        }
      }
    }

    if (chunkCounter === 1) {
      out.set(new Uint32Array(stack.buffer, 0, 8));
    } else {
      // Final merges with ROOT flag on the last one
      while (stackPos > 8) {
        stackPos -= 16;
        const isRoot = stackPos === 0;
        compress(IV, 0, stack, stackPos, isRoot ? out : stack, isRoot ? 0 : stackPos, true, 0, BLOCK_LEN, flags | PARENT | (isRoot ? ROOT : 0));
        if (!isRoot) stackPos += 8;
      }
    }

    return wordsToBytes(out).slice(0, outputLen);
  }

  function hashHex(input) {
    const bytes = hash(input);
    let hex = '';
    for (let i = 0; i < bytes.length; i++) {
      hex += bytes[i].toString(16).padStart(2, '0');
    }
    return hex;
  }

  function toHex(bytes) {
    return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
  }

  // Initialize WASM SIMD
  async function initSimd() {
    try {
      // Decode base64 and compile WASM - if SIMD isn't supported, compilation will fail
      const wasmBinary = Uint8Array.from(atob(WASM_SIMD_B64), c => c.charCodeAt(0));
      const wasmModule = await WebAssembly.compile(wasmBinary);
      const wasmInstance = await WebAssembly.instantiate(wasmModule);

      wasmCompress4x = wasmInstance.exports.compressChunks4x;
      wasmMem32 = new Uint32Array(wasmInstance.exports.memory.buffer);
      wasmSimdEnabled = true;
      return true;
    } catch (e) {
      return false;
    }
  }

  function isSimdEnabled() {
    return wasmSimdEnabled;
  }

  // ============================================
  // STREAMING API
  // ============================================

  // Helper to convert 32-byte key to 8 uint32 words (little-endian)
  function keyToWords(key) {
    if (key.length !== 32) {
      throw new Error('Key must be exactly 32 bytes');
    }
    const words = new Uint32Array(8);
    for (let i = 0; i < 8; i++) {
      words[i] = key[i * 4] |
                 (key[i * 4 + 1] << 8) |
                 (key[i * 4 + 2] << 16) |
                 (key[i * 4 + 3] << 24);
    }
    return words;
  }

  class Hasher {
    constructor(key = null, flags = 0) {
      // For keyed mode, key becomes the initial CV; otherwise use IV
      if (key !== null) {
        if (typeof key === 'string') {
          key = new TextEncoder().encode(key);
        }
        if (!(key instanceof Uint8Array)) {
          key = new Uint8Array(key);
        }
        this.keyWords = keyToWords(key);
        // Only add KEYED_HASH if not already in a derive key mode
        if (flags & (DERIVE_KEY_CONTEXT | DERIVE_KEY_MATERIAL)) {
          this.baseFlags = flags;
        } else {
          this.baseFlags = flags | KEYED_HASH;
        }
      } else {
        this.keyWords = IV;
        this.baseFlags = flags;
      }

      this.cv = new Uint32Array(8);
      this.cv.set(this.keyWords);
      this.blockBuffer = new Uint8Array(BLOCK_LEN);
      this.blockLen = 0;
      this.chunkLen = 0;
      this.chunkCounter = 0;
      this.cvStack = [];
    }

    update(data) {
      if (typeof data === 'string') {
        data = new TextEncoder().encode(data);
      }
      if (!(data instanceof Uint8Array)) {
        data = new Uint8Array(data);
      }

      let offset = 0;
      const len = data.length;

      while (offset < len) {
        // If block buffer is full, compress it first
        if (this.blockLen === BLOCK_LEN) {
          const isLastBlockOfChunk = (this.chunkLen === CHUNK_LEN);
          this._compressBlock(isLastBlockOfChunk);
          this.blockLen = 0;

          // If this completed a chunk, push CV and start new chunk
          if (isLastBlockOfChunk) {
            this._pushChunkCv();
            this._startNewChunk();
          }
        }

        const spaceInBlock = BLOCK_LEN - this.blockLen;
        const spaceInChunk = CHUNK_LEN - this.chunkLen;
        const spaceAvailable = Math.min(spaceInBlock, spaceInChunk);
        const bytesToTake = Math.min(spaceAvailable, len - offset);

        this.blockBuffer.set(data.subarray(offset, offset + bytesToTake), this.blockLen);
        this.blockLen += bytesToTake;
        this.chunkLen += bytesToTake;
        offset += bytesToTake;
      }

      return this;
    }

    finalize(outputLen = 32) {
      // Determine the root output node: CV, block, blockLen, flags
      let rootCv, rootBlock, rootBlockLen, rootFlags;
      const hasData = this.chunkCounter > 0 || this.chunkLen > 0;

      if (!hasData) {
        // Empty input case
        rootCv = this.keyWords;
        rootBlock = new Uint32Array(16); // zeros
        rootBlockLen = 0;
        rootFlags = this.baseFlags | CHUNK_START | CHUNK_END | ROOT;
      } else {
        const isOnlyChunk = this.chunkCounter === 0;

        if (isOnlyChunk) {
          // Single chunk - the root node is this chunk's final block
          // First, compress all complete blocks (without ROOT flag)
          // Then use the final partial block as the root node

          // Convert block buffer to words (same way as _compressBlock)
          rootBlock = new Uint32Array(16);
          for (let i = 0; i < this.blockLen; i++) {
            rootBlock[i >> 2] |= this.blockBuffer[i] << ((i & 3) * 8);
          }

          rootCv = this.cv; // Current CV (after any previous blocks in this chunk)
          rootBlockLen = this.blockLen;
          // CHUNK_START if this is the first block (chunkLen <= blockLen means no prior compressions)
          rootFlags = this.baseFlags | (this.chunkLen <= BLOCK_LEN ? CHUNK_START : 0) | CHUNK_END | ROOT;
        } else {
          // Multiple chunks - need to merge CVs and create parent root node
          // First, finalize the current chunk
          this._compressBlock(true, false); // CHUNK_END but not ROOT
          this.cvStack.push(new Uint32Array(this.cv));

          // Merge all CVs
          while (this.cvStack.length > 1) {
            const right = this.cvStack.pop();
            const left = this.cvStack.pop();

            if (this.cvStack.length === 0) {
              // This is the root merge - don't compress yet, save as root node
              rootCv = this.keyWords;
              rootBlock = new Uint32Array(16);
              rootBlock.set(left, 0);
              rootBlock.set(right, 8);
              rootBlockLen = BLOCK_LEN;
              rootFlags = this.baseFlags | PARENT | ROOT;
            } else {
              // Non-root merge
              const parentBlock = new Uint32Array(16);
              parentBlock.set(left, 0);
              parentBlock.set(right, 8);

              const merged = new Uint32Array(8);
              compress(this.keyWords, 0, parentBlock, 0, merged, 0, true, 0, BLOCK_LEN, this.baseFlags | PARENT);
              this.cvStack.push(merged);
            }
          }

          // Handle case where we have exactly one CV left (shouldn't happen in normal flow)
          if (this.cvStack.length === 1 && !rootCv) {
            const onlyCv = this.cvStack[0];
            rootCv = this.keyWords;
            rootBlock = new Uint32Array(16);
            rootBlock.set(onlyCv, 0);
            rootBlockLen = 32;
            rootFlags = this.baseFlags | PARENT | ROOT;
          }
        }
      }

      // Generate output using XOF mode
      if (outputLen <= 64) {
        // Can get all output from a single compression
        const out = new Uint32Array(16);
        compress(rootCv, 0, rootBlock, 0, out, 0, false, 0, rootBlockLen, rootFlags);
        return wordsToBytes(out).slice(0, outputLen);
      } else {
        // Need multiple compressions for extended output
        const result = new Uint8Array(outputLen);
        let offset = 0;
        let counter = 0;
        const out = new Uint32Array(16);

        while (offset < outputLen) {
          compress(rootCv, 0, rootBlock, 0, out, 0, false, counter, rootBlockLen, rootFlags);
          const bytes = wordsToBytes(out);
          const toCopy = Math.min(64, outputLen - offset);
          result.set(bytes.subarray(0, toCopy), offset);
          offset += 64;
          counter++;
        }

        return result;
      }
    }

    _pushChunkCv() {
      this.cvStack.push(new Uint32Array(this.cv));
      this.chunkCounter++;

      let tc = this.chunkCounter;
      while ((tc & 1) === 0 && this.cvStack.length >= 2) {
        const right = this.cvStack.pop();
        const left = this.cvStack.pop();

        const parentBlock = new Uint32Array(16);
        parentBlock.set(left, 0);
        parentBlock.set(right, 8);

        const merged = new Uint32Array(8);
        compress(this.keyWords, 0, parentBlock, 0, merged, 0, true, 0, BLOCK_LEN, this.baseFlags | PARENT);
        this.cvStack.push(merged);
        tc >>= 1;
      }
    }

    _startNewChunk() {
      this.cv.set(this.keyWords);
      this.chunkLen = 0;
    }

    _compressBlock(isLastBlock, isRoot = false) {
      let flags = this.baseFlags;

      if (this.chunkLen <= BLOCK_LEN) {
        flags |= CHUNK_START;
      }

      if (isLastBlock) {
        flags |= CHUNK_END;
      }

      if (isRoot) {
        flags |= ROOT;
      }

      const localBlockWords = new Uint32Array(16);
      for (let i = 0; i < this.blockLen; i++) {
        localBlockWords[i >> 2] |= this.blockBuffer[i] << ((i & 3) * 8);
      }

      compress(this.cv, 0, localBlockWords, 0, this.cv, 0, true, this.chunkCounter, this.blockLen, flags);
    }
  }

  function createHasher() {
    return new Hasher();
  }

  function createKeyedHasher(key) {
    if (typeof key === 'string') {
      key = new TextEncoder().encode(key);
    }
    if (!(key instanceof Uint8Array)) {
      key = new Uint8Array(key);
    }
    if (key.length !== 32) {
      throw new Error('Key must be exactly 32 bytes');
    }
    return new Hasher(key);
  }

  function hashKeyed(key, input, outputLen = 32) {
    const hasher = createKeyedHasher(key);
    hasher.update(input);
    return hasher.finalize(outputLen);
  }

  /**
   * Derive a key using BLAKE3 KDF
   * @param {string} context - Context string (domain separator)
   * @param {Uint8Array|string} keyMaterial - Input key material
   * @param {number} outputLen - Desired output length in bytes (default 32)
   * @returns {Uint8Array} Derived key
   */
  function deriveKey(context, keyMaterial, outputLen = 32) {
    // Step 1: Hash context string with DERIVE_KEY_CONTEXT flag to get context key
    // This uses IV as initial CV
    const contextHasher = new Hasher(null, DERIVE_KEY_CONTEXT);
    contextHasher.update(context);
    const contextKey = contextHasher.finalize(32);

    // Step 2: Hash key material using context key with DERIVE_KEY_MATERIAL flag
    const materialHasher = new Hasher(contextKey, DERIVE_KEY_MATERIAL);
    materialHasher.update(keyMaterial);
    return materialHasher.finalize(outputLen);
  }

  exports.hash = hash;
  exports.hashHex = hashHex;
  exports.toHex = toHex;
  exports.initSimd = initSimd;
  exports.isSimdEnabled = isSimdEnabled;
  exports.createHasher = createHasher;
  exports.createKeyedHasher = createKeyedHasher;
  exports.hashKeyed = hashKeyed;
  exports.deriveKey = deriveKey;
  exports.Hasher = Hasher;
  exports.IV = IV;
  exports.BLOCK_LEN = BLOCK_LEN;
  exports.CHUNK_LEN = CHUNK_LEN;
  exports.KEYED_HASH = KEYED_HASH;
  exports.DERIVE_KEY_CONTEXT = DERIVE_KEY_CONTEXT;
  exports.DERIVE_KEY_MATERIAL = DERIVE_KEY_MATERIAL;

  // Debug exports for SIMD testing
  exports._compress = compress;
  exports._processChunks4xSimd = processChunks4xSimd;
  exports._getWasmMem32 = () => wasmMem32;
  exports._simdCvs = () => simdCvs;

})(blake3Impl);


/**
 * @param {Buffer|Uint8Array|string} data
 * @returns {string} lowercase hex object id (32-byte BLAKE3 digest)
 */
export function objectId(data) {
  return blake3Impl.hashHex(data);
}

/**
 * @param {string} id
 * @param {Buffer|Uint8Array} bytes
 */
export function verifyObjectId(id, bytes) {
  return objectId(bytes) === id.toLowerCase();
}
