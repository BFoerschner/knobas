package main

import (
	"fmt"
	"net/http"
)

// Batch is retried, then parked.
type Batch struct {
	ID    string
	Tries int
}

func (b *Batch) Settle(client *http.Client) error {
	for b.Tries < 3 {
		b.Tries++
		if _, err := client.Get("http://localhost:8080/" + b.ID); err == nil {
			return nil
		}
	}
	return fmt.Errorf("parked %s\n", b.ID)
}
