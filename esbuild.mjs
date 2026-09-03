// esbuild 빌드 스크립트 (tech-stack.md §2)
// 산출물: CommonJS 단일 파일 dist/extension.cjs, external: ['vscode']
// .cjs 확장자를 명시하는 이유: package.json의 "type": "module" 아래에서도
// VS Code 확장 호스트가 require()로 로드할 수 있는 CommonJS로 명확히 고정하기 위해서다.
import { build, context } from 'esbuild';

const watch = process.argv.includes('--watch');

/** @type {import('esbuild').BuildOptions} */
const options = {
  entryPoints: ['src/extension.ts'],
  bundle: true,
  outfile: 'dist/extension.cjs',
  format: 'cjs',
  platform: 'node',
  target: 'node18',
  external: ['vscode'],
  sourcemap: true,
  minify: false,
  logLevel: 'info',
};

if (watch) {
  const ctx = await context(options);
  await ctx.watch();
  console.log('[esbuild] watching for changes...');
} else {
  await build(options);
}
