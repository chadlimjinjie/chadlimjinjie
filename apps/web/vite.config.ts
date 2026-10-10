import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'
import { pages } from './src/lib/seo.ts'

const MARKER = '<!--seo-->'

const headTags = ({ title, description }: (typeof pages)[string]) =>
  [
    `<title>${title}</title>`,
    `<meta name="description" content="${description}" />`,
    `<meta property="og:title" content="${title}" />`,
    `<meta property="og:description" content="${description}" />`,
  ].join('\n    ')

// Crawlers and link previews don't run JS, so each route gets its own HTML file with its head tags baked in.
function seo(): Plugin {
  let outDir: string
  return {
    name: 'seo',
    configResolved(config) {
      outDir = join(config.root, config.build.outDir)
    },
    transformIndexHtml(html) {
      if (!html.includes(MARKER)) throw new Error(`index.html is missing ${MARKER}`)
      return html.replace(MARKER, headTags(pages['/']))
    },
    closeBundle() {
      if (this.environment.config.command !== 'build') return
      const home = readFileSync(join(outDir, 'index.html'), 'utf8')
      for (const [path, page] of Object.entries(pages)) {
        if (path === '/') continue
        const html = home.replace(headTags(pages['/']), headTags(page))
        if (html === home) throw new Error(`seo: head tags for ${path} were not applied`)
        mkdirSync(join(outDir, path), { recursive: true })
        writeFileSync(join(outDir, path, 'index.html'), html)
      }
    },
  }
}

// https://vite.dev/config/
export default defineConfig({
  // tanstackRouter must come before react()
  plugins: [
    tanstackRouter({ target: 'react', autoCodeSplitting: true }),
    react(),
    tailwindcss(),
    seo(),
  ],
  resolve: {
    alias: { '@': new URL('./src', import.meta.url).pathname },
  },
})
