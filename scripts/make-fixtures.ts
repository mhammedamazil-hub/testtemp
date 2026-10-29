import { chunkText } from '../src/lib/text';
import { buildBm25Index, searchBm25 } from '../src/lib/bm25';
import { mkdirSync, writeFileSync } from 'node:fs';

const chunkCases = [
  {
    name: 'paragraphs',
    text: 'Alpha beta gamma delta.\n\nEpsilon zeta eta theta.\n\nIota kappa lambda mu.',
    chunkSize: 60,
    chunkOverlap: 10,
  },
  {
    name: 'exact-fit',
    text: 'one two three\nfour five six\nseven eight nine',
    chunkSize: 28,
    chunkOverlap: 0,
  },
  {
    name: 'overlap',
    text: [
      'line-one-aaaaaaaaaaaaaaaaaaaa',
      'line-two-bbbbbbbbbbbbbbbbbbbb',
      'line-three-cccccccccccccccccc',
      'line-four-ddddddddddddddddddd',
      'line-five-eeeeeeeeeeeeeeeeeeee',
      'line-six-fffffffffffffffffffff',
    ].join('\n'),
    chunkSize: 90,
    chunkOverlap: 35,
  },
  {
    name: 'no-trailing-duplicate',
    text: 'aaaaaaaaaa\nbbbbbbbbbb\ncccccccccc\ndddddddddd\neeeeeeeeee',
    chunkSize: 80,
    chunkOverlap: 12,
  },
  {
    name: 'long-line',
    text: `${'x'.repeat(150)}\nshort line`,
    chunkSize: 80,
    chunkOverlap: 0,
  },
  {
    name: 'blank-lines',
    text: 'first\n\n\nsecond\n\nthird',
    chunkSize: 200,
    chunkOverlap: 0,
  },
];

const chunkFixture = chunkCases.map((c) => ({
  name: c.name,
  text: c.text,
  chunkSize: c.chunkSize,
  chunkOverlap: c.chunkOverlap,
  expected: chunkText(c.text, { chunkSize: c.chunkSize, chunkOverlap: c.chunkOverlap }).map((chunk) => ({
    text: chunk.text,
    startLine: chunk.startLine,
    endLine: chunk.endLine,
    tokens: chunk.tokens,
  })),
}));

const rankDocs = [
  { id: 'a', text: 'The market share of the product grew by 40 percent in 2023.' },
  { id: 'b', text: 'Customer satisfaction surveys show high approval among enterprise buyers.' },
  { id: 'c', text: 'The product market share was 80 percent according to a rival analyst.' },
  { id: 'd', text: 'Unrelated text about gardening and soil pH levels.' },
];
const rankQueries = ['market share percent', 'customer approval', 'gardening soil', 'nonexistent zebra'];
const rankingFixture = rankQueries.map((query) => {
  const index = buildBm25Index(rankDocs);
  return {
    docs: rankDocs,
    query,
    expected: searchBm25(index, query, 10).map((r) => ({ id: r.id, score: Number(r.score.toFixed(6)) })),
  };
});

mkdirSync('core/tests/fixtures', { recursive: true });
writeFileSync('core/tests/fixtures/chunking.json', JSON.stringify(chunkFixture, null, 2) + '\n');
writeFileSync('core/tests/fixtures/ranking.json', JSON.stringify(rankingFixture, null, 2) + '\n');
console.log('chunk cases:', chunkFixture.map((c) => `${c.name}:${c.expected.length}`).join(' '));
console.log(JSON.stringify(chunkFixture[2], null, 1));
console.log(JSON.stringify(rankingFixture[0], null, 1));
