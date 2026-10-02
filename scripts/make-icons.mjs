// 图标源生成脚本（scripts/make-icons.mjs）
// 用途：生成 assets/icon-source.png（1024×1024，RGBA），供 `npx tauri icon` 生成全套平台图标
// 依赖：仅 Node 内置模块（node:zlib + 手写 PNG 编码），不引入任何图形库
// 设计与项目视觉一致：深灰圆角底 + 纸白页面 + 灰字行 + 墨蓝书签（无渐变/无阴影）
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** 画布边长（像素）；Tauri 建议 ≥1024 */
const SIZE = 1024;
/** 输出路径：<项目根>/assets/icon-source.png */
const OUT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  'assets',
  'icon-source.png',
);

// ---------------------------------------------------------------------------
// PNG 编码基础设施（最小实现：RGBA8、无隔行、filter=0）
// ---------------------------------------------------------------------------

/** CRC32 查表（PNG chunk 校验必需） */
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

/**
 * 计算 CRC32。
 * @param {Buffer} buf 输入字节
 * @returns {number} 无符号 32 位校验值
 */
function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) {
    c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

/**
 * 组装 PNG chunk：长度(4) + 类型(4) + 数据 + CRC(4)。
 * @param {string} type 四字符类型（IHDR/IDAT/IEND）
 * @param {Buffer} data 载荷
 * @returns {Buffer} 完整 chunk
 */
function chunk(type, data) {
  const typeBuf = Buffer.from(type, 'ascii');
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])));
  return Buffer.concat([len, typeBuf, data, crc]);
}

/**
 * 将 RGBA 像素编码为 PNG Buffer。
 * @param {Uint8Array} rgba 长度 = SIZE*SIZE*4 的像素数据
 * @returns {Buffer} PNG 文件内容
 */
function encodePng(rgba) {
  const stride = SIZE * 4;
  // 原始扫描线：每行前置 1 字节 filter=0
  const raw = Buffer.alloc((stride + 1) * SIZE);
  for (let y = 0; y < SIZE; y += 1) {
    raw[y * (stride + 1)] = 0;
    Buffer.from(rgba.buffer, y * stride, stride).copy(raw, y * (stride + 1) + 1);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(SIZE, 0);
  ihdr.writeUInt32BE(SIZE, 4);
  ihdr[8] = 8; // 位深
  ihdr[9] = 6; // 颜色类型：RGBA
  ihdr[10] = 0; // 压缩方式
  ihdr[11] = 0; // 过滤方式
  ihdr[12] = 0; // 非隔行
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---------------------------------------------------------------------------
// 矢量图形绘制（圆角矩形 SDF + 1px 抗锯齿）
// ---------------------------------------------------------------------------

/**
 * 圆角矩形有符号距离场（SDF）。
 * @returns {number} <0 在形状内；>0 在形状外（距离）
 */
function roundedRectSdf(px, py, cx, cy, halfW, halfH, radius) {
  const qx = Math.abs(px - cx) - halfW + radius;
  const qy = Math.abs(py - cy) - halfH + radius;
  return (
    Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) +
    Math.min(Math.max(qx, qy), 0) -
    radius
  );
}

/**
 * SDF → 覆盖率（0..1），1px 平滑过渡实现抗锯齿。
 * @param {number} d SDF 距离
 */
function coverage(d) {
  return Math.min(Math.max(0.5 - d, 0), 1);
}

/**
 * 将一层图形以 src-over 混合写入画布。
 * @param {Float32Array} acc 画布（RGBA，0..1 浮点，预乘直存）
 * @param {(x:number,y:number)=>number} alphaAt 覆盖率函数
 * @param {[number,number,number]} rgb 颜色（0..255）
 */
function paint(acc, alphaAt, rgb) {
  const [r, g, b] = rgb.map((v) => v / 255);
  for (let y = 0; y < SIZE; y += 1) {
    for (let x = 0; x < SIZE; x += 1) {
      const a = alphaAt(x + 0.5, y + 0.5);
      if (a <= 0) continue;
      const i = (y * SIZE + x) * 4;
      const inv = 1 - a;
      // src-over（直通 alpha）
      const outA = a + acc[i + 3] * inv;
      acc[i] = (r * a + acc[i] * acc[i + 3] * inv) / (outA || 1);
      acc[i + 1] = (g * a + acc[i + 1] * acc[i + 3] * inv) / (outA || 1);
      acc[i + 2] = (b * a + acc[i + 2] * acc[i + 3] * inv) / (outA || 1);
      acc[i + 3] = outA;
    }
  }
}

// 画布：初始全透明
const acc = new Float32Array(SIZE * SIZE * 4);

// 层 1：深灰圆角底（满出血，圆角 200）
paint(acc, (x, y) => coverage(roundedRectSdf(x, y, 512, 512, 512, 512, 200)), [42, 44, 48]);
// 层 2：纸白页面（居中偏上）
paint(acc, (x, y) => coverage(roundedRectSdf(x, y, 512, 510, 250, 300, 36)), [250, 249, 247]);
// 层 3：四条灰色文字行（留出右侧书签区域）
for (const [top, bottom] of [
  [317, 343],
  [417, 443],
  [517, 543],
  [617, 643],
]) {
  paint(
    acc,
    (x, y) => coverage(roundedRectSdf(x, y, 465, (top + bottom) / 2, 135, (bottom - top) / 2, 13)),
    [201, 195, 182],
  );
}
// 层 4：墨蓝书签（从页面顶部垂下，位于右侧空白区）
paint(acc, (x, y) => coverage(roundedRectSdf(x, y, 678, 330, 38, 120, 10)), [59, 110, 165]);

// 层 4b：书签底部缺口（透明三角）——用“反向覆盖”擦除，保持简洁图形语言
for (let y = 0; y < SIZE; y += 1) {
  for (let x = 0; x < SIZE; x += 1) {
    // 三角形：顶点 (678, 380)，底边 y=450 从 x=648..708
    const t = (y - 380) / 70;
    if (t >= 0 && t <= 1) {
      const half = 30 * (1 - t);
      if (x >= 678 - half && x <= 678 + half) {
        const i = (y * SIZE + x) * 4;
        acc[i + 3] = 0;
      }
    }
  }
}

// 浮点画布 → RGBA8
const rgba = new Uint8Array(SIZE * SIZE * 4);
for (let i = 0; i < rgba.length; i += 4) {
  rgba[i] = Math.round(Math.min(Math.max(acc[i], 0), 1) * 255);
  rgba[i + 1] = Math.round(Math.min(Math.max(acc[i + 1], 0), 1) * 255);
  rgba[i + 2] = Math.round(Math.min(Math.max(acc[i + 2], 0), 1) * 255);
  rgba[i + 3] = Math.round(Math.min(Math.max(acc[i + 3], 0), 1) * 255);
}

mkdirSync(path.dirname(OUT), { recursive: true });
writeFileSync(OUT, encodePng(rgba));
console.log(`图标源已生成：${OUT}（1024×1024）。下一步：npx tauri icon assets/icon-source.png`);
