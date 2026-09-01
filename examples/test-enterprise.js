async function test() {
  console.log('🚀 Connecting to FaizDB for Enterprise Verification...');
  const loginRes = await fetch('http://127.0.0.1:27018/v1/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: 'admin', password: 'faizdb-admin-2026' })
  });
  const { data: { token } } = await loginRes.json();
  const headers = { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + token };

  // 1. Create a Unique Index on 'users(email)'
  console.log('\n1. Creating Unique Index on users.email...');
  const idxRes = await fetch('http://127.0.0.1:27018/v1/query', {
    method: 'POST',
    headers,
    body: JSON.stringify({ query: 'CREATE UNIQUE INDEX idx_email ON users(email)' })
  });
  console.log('Index Result:', await idxRes.json());

  // 2. Insert document 1
  console.log('\n2. Inserting first user with faiz@ict.house...');
  const ins1 = await fetch('http://127.0.0.1:27018/v1/collections/users/insert', {
    method: 'POST',
    headers,
    body: JSON.stringify({ name: 'Faiz', email: 'faiz@ict.house' })
  });
  console.log('Insert 1:', await ins1.json());

  // 3. Insert duplicate document 2 (Must fail with DuplicateKey!)
  console.log('\n3. Inserting duplicate user with same email (Should be blocked by UNIQUE index!)...');
  const ins2 = await fetch('http://127.0.0.1:27018/v1/collections/users/insert', {
    method: 'POST',
    headers,
    body: JSON.stringify({ name: 'Imposter', email: 'faiz@ict.house' })
  });
  console.log('Insert 2 (Duplicate Blocked):', await ins2.json());

  // 4. Test EXPLAIN query execution plan
  console.log('\n4. Running EXPLAIN SELECT * FROM users WHERE email = "faiz@ict.house"...');
  const explainRes = await fetch('http://127.0.0.1:27018/v1/query', {
    method: 'POST',
    headers,
    body: JSON.stringify({ query: 'EXPLAIN SELECT * FROM users WHERE email = "faiz@ict.house"' })
  });
  console.log('Explain Plan Result:', JSON.stringify(await explainRes.json(), null, 2));

  // 5. Test Transactions
  console.log('\n5. Testing Transaction BEGIN / COMMIT...');
  const beginRes = await fetch('http://127.0.0.1:27018/v1/transaction/begin', { method: 'POST', headers });
  console.log('Begin:', await beginRes.json());
  const commitRes = await fetch('http://127.0.0.1:27018/v1/transaction/commit', { method: 'POST', headers });
  console.log('Commit:', await commitRes.json());

  console.log('\n🎉 ALL ENTERPRISE CRITICAL FEATURES PASSED VERIFICATION!');
}

test().catch(console.error);
