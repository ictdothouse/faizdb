import express from 'express';
import cors from 'cors';
import { MongoClient } from 'mongodb';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const app = express();
const PORT = 3000;
const FAIZDB_URI = process.env.FAIZDB_URI || 'mongodb://127.0.0.1:27017';

app.use(cors());
app.use(express.json());
app.use(express.static(path.join(__dirname, 'public')));

let db, usersCollection;

// Connect to FaizDB via official MongoDB Wire Protocol
async function initFaizDB() {
  try {
    console.log(`Connecting to FaizDB at ${FAIZDB_URI}...`);
    const client = new MongoClient(FAIZDB_URI, { serverSelectionTimeoutMS: 3000 });
    await client.connect();
    db = client.db('enterprise_app');
    usersCollection = db.collection('customers');
    console.log('✅ Successfully connected to FaizDB MongoDB Wire Protocol (Port 27017)!');
  } catch (err) {
    console.error('❌ Failed to connect to FaizDB:', err.message);
  }
}

// API Route: Get all customers
app.get('/api/customers', async (req, res) => {
  try {
    const customers = await usersCollection.find({}).toArray();
    res.json({ success: true, count: customers.length, data: customers });
  } catch (err) {
    res.status(500).json({ success: false, error: err.message });
  }
});

// API Route: Create customer
app.post('/api/customers', async (req, res) => {
  try {
    const { name, email, plan, revenue } = req.body;
    const newCustomer = {
      name,
      email,
      plan: plan || 'Pro',
      revenue: Number(revenue) || 0,
      created_at: new Date()
    };
    const result = await usersCollection.insertOne(newCustomer);
    res.status(201).json({ success: true, id: result.insertedId, data: newCustomer });
  } catch (err) {
    res.status(500).json({ success: false, error: err.message });
  }
});

// API Route: Run High-Performance Aggregation Analytics
app.get('/api/analytics', async (req, res) => {
  try {
    const pipeline = [
      { $match: { revenue: { $gt: 0 } } },
      { $group: { _id: '$plan', totalRevenue: { $sum: '$revenue' }, count: { $sum: 1 } } },
      { $sort: { totalRevenue: -1 } }
    ];
    const analytics = await usersCollection.aggregate(pipeline).toArray();
    res.json({ success: true, data: analytics });
  } catch (err) {
    res.status(500).json({ success: false, error: err.message });
  }
});

// Start Server
app.listen(PORT, async () => {
  await initFaizDB();
  console.log(`🚀 Node.js App running at http://localhost:${PORT}`);
});
