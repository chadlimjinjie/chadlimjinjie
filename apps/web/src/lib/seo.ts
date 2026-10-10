const SITE = 'Chad Lim'

// ponytail: values go into HTML unescaped (see vite.config.ts). Keep `"`, `<` and `&` out, or add escaping there.
export const pages: Record<string, { title: string; description: string }> = {
  '/': { title: SITE, description: 'Personal site of Chad Lim.' },
  '/about': { title: `About | ${SITE}`, description: 'About Chad Lim.' },
  '/location': {
    title: `Location | ${SITE}`,
    description: 'Where Chad Lim is right now, on a live map.',
  },
}
