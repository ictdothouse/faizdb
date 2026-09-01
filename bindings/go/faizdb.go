package faizdb

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
)

// Client is the FaizDB client
type Client struct {
	endpoint string
	http     *http.Client
}

// NewClient creates a new FaizDB client connection
func NewClient(endpoint string) *Client {
	if endpoint == "" {
		endpoint = "http://localhost:27018"
	}
	return &Client{
		endpoint: strings.TrimRight(endpoint, "/"),
		http:     &http.Client{},
	}
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
	resp, err := c.http.Post(c.endpoint+"/v1/query", "application/json", bytes.NewReader(reqBody))
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
	resp, err := col.client.http.Post(
		fmt.Sprintf("%s/v1/collections/%s/insert", col.client.endpoint, col.name),
		"application/json",
		bytes.NewReader(reqBody),
	)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	var apiResp struct {
		Success bool              `json:"success"`
		Data    map[string]string `json:"data"`
		Error   string            `json:"error"`
	}
	body, _ := io.ReadAll(resp.Body)
	if err := json.Unmarshal(body, &apiResp); err != nil {
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

// VectorSearch executes vector similarity search
func (col *Collection) VectorSearch(vector []float32, topK int) (interface{}, error) {
	vecBytes, _ := json.Marshal(vector)
	query := fmt.Sprintf("FIND %s VECTOR NEAR %s TOP %d", col.name, string(vecBytes), topK)
	return col.client.Query(query)
}
