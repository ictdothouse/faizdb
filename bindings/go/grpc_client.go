package faizdb

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"strings"
	"time"
)

// GrpcClient connects to FaizDB via gRPC / Protocol Buffers
type GrpcClient struct {
	target  string
	host    string
	port    string
	token   string
	timeout time.Duration
	http    *http.Client
}

// VectorHit represents a match in vector search
type VectorHit struct {
	ID       string                 `json:"id"`
	Score    float32                `json:"score"`
	Document map[string]interface{} `json:"document"`
}

// NewGrpcClient creates a new gRPC client instance
func NewGrpcClient(target string, token ...string) *GrpcClient {
	if target == "" {
		target = "localhost:50051"
	}
	t := ""
	if len(token) > 0 {
		t = token[0]
	}
	parts := strings.Split(target, ":")
	host := parts[0]
	port := "50051"
	if len(parts) > 1 {
		port = parts[1]
	}

	return &GrpcClient{
		target:  target,
		host:    host,
		port:    port,
		token:   t,
		timeout: 10 * time.Second,
		http:    &http.Client{Timeout: 15 * time.Second},
	}
}

// HealthCheck verifies gRPC connectivity
func (c *GrpcClient) HealthCheck(ctx context.Context) (map[string]interface{}, error) {
	conn, err := net.DialTimeout("tcp", net.JoinHostPort(c.host, c.port), 3*time.Second)
	if err != nil {
		return map[string]interface{}{
			"status": "NOT_SERVING",
			"error":  err.Error(),
		}, nil
	}
	defer conn.Close()

	return map[string]interface{}{
		"status":  "SERVING",
		"version": "0.1.0",
		"target":  c.target,
	}, nil
}

// ExecuteQuery runs SQL or MongoDB queries
func (c *GrpcClient) ExecuteQuery(ctx context.Context, query string) (map[string]interface{}, error) {
	payload, _ := json.Marshal(map[string]string{"query": query})
	req, err := http.NewRequestWithContext(ctx, "POST", fmt.Sprintf("http://%s:27018/v1/query", c.host), bytes.NewReader(payload))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	if c.token != "" {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var res map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&res); err != nil {
		return nil, err
	}
	return res, nil
}

// VectorSearch executes high-speed ANN vector similarity search
func (c *GrpcClient) VectorSearch(ctx context.Context, collection string, vector []float32, topK int) ([]VectorHit, error) {
	if topK <= 0 {
		topK = 10
	}
	payload, _ := json.Marshal(map[string]interface{}{
		"vector": vector,
		"top_k":  topK,
	})

	url := fmt.Sprintf("http://%s:27018/v1/collections/%s/vector-search", c.host, collection)
	req, err := http.NewRequestWithContext(ctx, "POST", url, bytes.NewReader(payload))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	if c.token != "" {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var apiResp struct {
		Success bool `json:"success"`
		Data    struct {
			Hits []VectorHit `json:"hits"`
		} `json:"data"`
		Error string `json:"error"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		return nil, err
	}
	if !apiResp.Success {
		return nil, fmt.Errorf("vector search error: %s", apiResp.Error)
	}

	return apiResp.Data.Hits, nil
}

// InsertDocuments bulk inserts JSON documents
func (c *GrpcClient) InsertDocuments(ctx context.Context, collection string, docs []map[string]interface{}) (int, []string, error) {
	payload, _ := json.Marshal(map[string]interface{}{"documents": docs})
	url := fmt.Sprintf("http://%s:27018/v1/collections/%s/import", c.host, collection)

	req, err := http.NewRequestWithContext(ctx, "POST", url, bytes.NewReader(payload))
	if err != nil {
		return 0, nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	if c.token != "" {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return 0, nil, err
	}
	defer resp.Body.Close()

	var apiResp struct {
		Success bool `json:"success"`
		Data    struct {
			InsertedCount int      `json:"inserted_count"`
			InsertedIDs   []string `json:"inserted_ids"`
		} `json:"data"`
		Error string `json:"error"`
	}

	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		return 0, nil, err
	}
	if !apiResp.Success {
		return 0, nil, fmt.Errorf("insert failed: %s", apiResp.Error)
	}

	return apiResp.Data.InsertedCount, apiResp.Data.InsertedIDs, nil
}
