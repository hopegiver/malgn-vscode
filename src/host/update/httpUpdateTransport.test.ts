import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { HttpUpdateTransport, UpdateArtifactSchemeRejectedError, UpdateTransportHttpError } from './httpUpdateTransport.js';

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
  vi.restoreAllMocks();
});

describe('HttpUpdateTransport.fetchManifest', () => {
  it('https://<authority>/manifest.json을 GET해 JSON을 반환한다', async () => {
    const fetchMock = vi.fn(async (url: string) => {
      expect(url).toBe('https://updates.example.com/manifest.json');
      return new Response(JSON.stringify({ version: '1.0.0' }), { status: 200 });
    });
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const transport = new HttpUpdateTransport();
    const manifest = await transport.fetchManifest('updates.example.com');
    expect(manifest).toEqual({ version: '1.0.0' });
  });

  it('HTTP 실패 상태면 던진다', async () => {
    globalThis.fetch = vi.fn(async () => new Response('', { status: 500 })) as unknown as typeof fetch;
    const transport = new HttpUpdateTransport();
    await expect(transport.fetchManifest('updates.example.com')).rejects.toBeInstanceOf(UpdateTransportHttpError);
  });
});

describe('HttpUpdateTransport.fetchArtifact', () => {
  it('아티팩트를 Buffer로 반환한다', async () => {
    const bytes = new Uint8Array([1, 2, 3, 4]);
    globalThis.fetch = vi.fn(async () => new Response(bytes, { status: 200 })) as unknown as typeof fetch;
    const transport = new HttpUpdateTransport();
    const buf = await transport.fetchArtifact('https://updates.example.com/artifact.bin');
    expect(Buffer.from(buf)).toEqual(Buffer.from(bytes));
  });

  it('https가 아닌 스킴은 거부한다(fetch를 호출하지 않는다)', async () => {
    const fetchMock = vi.fn();
    globalThis.fetch = fetchMock as unknown as typeof fetch;
    const transport = new HttpUpdateTransport();
    await expect(transport.fetchArtifact('http://updates.example.com/artifact.bin')).rejects.toBeInstanceOf(
      UpdateArtifactSchemeRejectedError
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('HTTP 실패 상태면 던진다', async () => {
    globalThis.fetch = vi.fn(async () => new Response('', { status: 404 })) as unknown as typeof fetch;
    const transport = new HttpUpdateTransport();
    await expect(transport.fetchArtifact('https://updates.example.com/artifact.bin')).rejects.toBeInstanceOf(UpdateTransportHttpError);
  });
});
