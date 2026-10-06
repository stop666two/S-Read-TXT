// 更新检查本地桩服务器：提供 GitHub Release 同构 JSON 与安装包下载，供 E2E 验证。
// 用法：const stub = await startStubUpdateServer(); stub.port / stub.sha256 / stub.requestCount / stub.close()
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';

/** 安装包内容（固定 256KB 可预测载荷） */
function buildPayload() {
  const chunk = Buffer.from('S-READ-TXT-UPDATE-STUB-PAYLOAD-0123456789\n', 'utf8');
  const size = 256 * 1024;
  const buffer = Buffer.alloc(size);
  for (let offset = 0; offset < size; offset += chunk.length) {
    chunk.copy(buffer, offset, 0, Math.min(chunk.length, size - offset));
  }
  return buffer;
}

const sha256Hex = (buffer) => createHash('sha256').update(buffer).digest('hex');

/**
 * 启动桩服务器。
 * @param {{ port?: number, version?: string }} [options]
 * @returns {Promise<{ port: number, version: string, sha256: string, payloadLength: number, requestCount: () => number, close: () => Promise<void> }>}
 */
export function startStubUpdateServer(options = {}) {
  const version = options.version ?? 'v0.0.2-beta';
  const payload = buildPayload();
  const sha256 = sha256Hex(payload);
  const badDigest = sha256Hex(Buffer.from('tampered-content'));
  let requests = 0;

  const server = createServer((req, res) => {
    requests += 1;
    const url = (req.url ?? '/').split('?')[0];
    if (url === '/latest.json' || url === '/latest-bad.json') {
      const payloadUrl = `http://127.0.0.1:${server.address().port}/download/setup.exe`;
      const digest = url === '/latest-bad.json' ? `sha256:${badDigest}` : `sha256:${sha256}`;
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(
        JSON.stringify({
          tag_name: version,
          html_url: `http://127.0.0.1:${server.address().port}/release-page`,
          assets: [
            { name: 'source.zip', browser_download_url: `${payloadUrl}.zip` },
            {
              name: `S-Read-TXT_${version.replace(/^v/, '')}_x64-setup.exe`,
              browser_download_url: payloadUrl,
              digest,
              size: payload.length,
            },
          ],
        }),
      );
      return;
    }
    if (url === '/download/setup.exe') {
      res.writeHead(200, { 'content-type': 'application/octet-stream' });
      res.end(payload);
      return;
    }
    res.writeHead(404, { 'content-type': 'text/plain' });
    res.end('not found');
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(options.port ?? 0, '127.0.0.1', () => {
      resolve({
        port: server.address().port,
        version,
        sha256,
        payloadLength: payload.length,
        requestCount: () => requests,
        close: () =>
          new Promise((done) => {
            server.close(() => done());
          }),
      });
    });
  });
}
