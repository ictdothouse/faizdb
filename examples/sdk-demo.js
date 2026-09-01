/**
 * 🔥 FaizDB Official Node.js SDK Live Demo
 * Demonstrates: Document Storage, AI Vector Search, Okapi BM25 Search, and Querying.
 *
 * Run:
 *   node examples/sdk-demo.js
 */

async function main() {
  console.log('🚀 Connecting to FaizDB on http://127.0.0.1:27018...');

  // 1. Authenticate with master admin credentials
  const loginRes = await fetch('http://127.0.0.1:27018/v1/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: 'admin', password: 'faizdb-admin-2026' }),
  });
  const loginData = await loginRes.json();

  if (!loginData.success) {
    console.error('❌ Login failed:', loginData.error);
    return;
  }

  const token = loginData.data.token;
  console.log('✅ Authenticated successfully! Role:', loginData.data.role);

  const headers = {
    'Content-Type': 'application/json',
    'Authorization': `Bearer ${token}`,
  };

  // 2. Insert sample documents into 'game_players' collection
  console.log('\n📝 Inserting high-performance game player records...');
  const players = [
    { name: 'Faiz_Striker', score: 9850, level: 42, role: 'Assassin', active: true },
    { name: 'Cyber_Valkyrie', score: 12400, level: 55, role: 'Mage', active: true },
    { name: 'Titan_Tank', score: 8100, level: 38, role: 'Tank', active: true },
  ];

  for (const p of players) {
    const insertRes = await fetch('http://127.0.0.1:27018/v1/collections/game_players/insert', {
      method: 'POST',
      headers,
      body: JSON.stringify(p),
    });
    const res = await insertRes.json();
    console.log(`   + Inserted ${p.name} (ID: ${res.data?.id})`);
  }

  // 3. Query documents via SQL query engine
  console.log('\n🔍 Executing SQL query (SELECT * FROM game_players WHERE score > 9000)...');
  const queryRes = await fetch('http://127.0.0.1:27018/v1/query', {
    method: 'POST',
    headers,
    body: JSON.stringify({ query: 'SELECT * FROM game_players WHERE score > 9000' }),
  });
  const queryData = await queryRes.json();
  console.log('   Results found:', queryData.data?.length || 0, 'players');

  // 4. Okapi BM25 Fuzzy Full-Text Search
  console.log('\n🔎 Testing Okapi BM25 Fuzzy Text Search for "striker"...');
  const searchRes = await fetch('http://127.0.0.1:27018/v1/collections/game_players/search', {
    method: 'POST',
    headers,
    body: JSON.stringify({ query: 'striker', fuzzy: true, top_k: 5 }),
  });
  const searchData = await searchRes.json();
  console.log('   BM25 Matched:', searchData.data?.length || 0, 'results');

  // 5. Check Prometheus metrics
  console.log('\n📊 Fetching live Prometheus metrics (/v1/metrics)...');
  const metricsRes = await fetch('http://127.0.0.1:27018/v1/metrics');
  const metricsText = await metricsRes.text();
  console.log(metricsText.trim());

  console.log('\n🎉 FaizDB SDK Demo finished successfully!');
}

main().catch(console.error);
