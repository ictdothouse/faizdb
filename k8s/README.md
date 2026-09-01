# ☸️ FaizDB Cloud-Native Kubernetes Deployment

Deploy a fault-tolerant, 3-node High-Availability FaizDB cluster on Kubernetes (EKS, GKE, AKS, or on-premise K8s).

---

## 🚀 Quick Deployment (3 Steps)

### 1. Create Secrets for Master Admin & JWT
```bash
kubectl create secret generic faizdb-credentials \
  --from-literal=admin-user=admin \
  --from-literal=admin-password=faizdb-super-secret-2026 \
  --from-literal=jwt-secret=faizdb-master-jwt-key-256bit
```

### 2. Apply the StatefulSet & Services
```bash
kubectl apply -f k8s/statefulset.yaml
```

### 3. Verify Cluster Health
```bash
# Check running pods
kubectl get pods -l app=faizdb

# Port forward Studio & HTTP API to local machine
kubectl port-forward svc/faizdb-service 27018:27018 27017:27017
```

Connect your application via `mongodb://faizdb-service.default.svc.cluster.local:27017` or `http://faizdb-service.default.svc.cluster.local:27018`.
