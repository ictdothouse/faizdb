package faizdb

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// Client is the FaizDB client
type Client struct {
	endpoint string
	token    string
	http     *http.Client
}

// NewClient creates a new FaizDB client connection
func NewClient(endpoint string, token ...string) *Client {
	if endpoint == "" {
		endpoint = "http://localhost:27018"
	}
	t := ""
	if len(token) > 0 {
		t = token[0]
	}
	return &Client{
		endpoint: strings.TrimRight(endpoint, "/"),
		token:    t,
		http:     &http.Client{Timeout: 30 * time.Second},
	}
}

// Login authenticates with master or user credentials
func (c *Client) Login(username, password string) (string, error) {
	payload, _ := json.Marshal(map[string]string{
		"username": username,
		"password": password,
	})
	resp, err := c.http.Post(c.endpoint+"/v1/auth/login", "application/json", bytes.NewReader(payload))
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	var apiResp struct {
		Success bool `json:"success"`
		Data    struct {
			Token string `json:"token"`
			Role  string `json:"role"`
		} `json:"data"`
		Error string `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		return "", err
	}
	if !apiResp.Success {
		return "", fmt.Errorf("login failed: %s", apiResp.Error)
	}
	c.token = apiResp.Data.Token
	return c.token, nil
}

// Collection returns a handle to a collection
func (c *Client) Collection(name string) *Collection {
	return &Collection{
		client: c,
		name:   name,
	}
}

// Query executes any SQL, MongoDB, or FaizQL query string
func (c *Client) Query(queryString string) (interface{}, error) {
	reqBody, _ := json.Marshal(map[string]string{"query": queryString})
	req, err := http.NewRequest("POST", c.endpoint+"/v1/query", bytes.NewReader(reqBody))
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

	body, _ := io.ReadAll(resp.Body)
	var apiResp struct {
		Success bool        `json:"success"`
		Data    interface{} `json:"data"`
		Error   string      `json:"error"`
	}
	if err := json.Unmarshal(body, &apiResp); err != nil {
		return nil, err
	}
	if !apiResp.Success {
		return nil, fmt.Errorf("query error: %s", apiResp.Error)
	}
	return apiResp.Data, nil
}

// Collection handle
type Collection struct {
	client *Client
	name   string
}

// Insert inserts a document
func (col *Collection) Insert(doc map[string]interface{}) (string, error) {
	reqBody, _ := json.Marshal(doc)
	req, err := http.NewRequest(
		"POST",
		fmt.Sprintf("%s/v1/collections/%s/insert", col.client.endpoint, col.name),
		bytes.NewReader(reqBody),
	)
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	if col.client.token != "" {
		req.Header.Set("Authorization", "Bearer "+col.client.token)
	}

	resp, err := col.client.http.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	var apiResp struct {
		Success bool              `json:"success"`
		Data    map[string]string `json:"data"`
		Error   string            `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		return "", err
	}
	if !apiResp.Success {
		return "", fmt.Errorf("insert failed: %s", apiResp.Error)
	}
	return apiResp.Data["id"], nil
}

// Find executes a find query
func (col *Collection) Find(filter map[string]interface{}) (interface{}, error) {
	filterBytes, _ := json.Marshal(filter)
	query := fmt.Sprintf("db.%s.find(%s)", col.name, string(filterBytes))
	return col.client.Query(query)
}

// VectorSearch runs high-speed HNSW similarity search
func (col *Collection) VectorSearch(vector []float32, topK int) (interface{}, error) {
	vecJson, _ := json.Marshal(vector)
	query := fmt.Sprintf("FIND %s VECTOR NEAR %s TOP %d", col.name, string(vecJson), topK)
	return col.client.Query(query)
}
