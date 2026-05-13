# JS client (Next.js)

A [Next.js](https://nextjs.org/) app, scaffolded from `create-next-app`.

## Running

Copy `.env.local.example` to `.env.local` and fill in the RPC endpoints.

```bash
cp .env.local.example .env.local
pnpm install
pnpm dev
```

Open <http://localhost:3000>. Editing `pages/index.tsx` reloads the page automatically.

API routes live under `pages/api`. The default route is `pages/api/hello.ts`, reachable at <http://localhost:3000/api/hello>.

The app uses `next/font` to optimize and load Inter.

## Deploy

The easiest way to deploy a Next.js app is on [Vercel](https://vercel.com/new). See [Next.js deployment docs](https://nextjs.org/docs/deployment) for alternatives.
