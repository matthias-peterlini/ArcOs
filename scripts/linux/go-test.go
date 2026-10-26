package main

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"sync"
	"time"
)

func main() {
	var wg sync.WaitGroup
	var mu sync.Mutex
	sum := 0
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func(k int) {
			defer wg.Done()
			s := 0
			for j := 0; j < 200000; j++ {
				s += j % (k + 2)
			}
			mu.Lock()
			sum += s
			mu.Unlock()
		}(i)
	}
	wg.Wait()
	fmt.Println("gt: goroutines sum", sum)
	t0 := time.Now()
	time.Sleep(100 * time.Millisecond)
	fmt.Println("gt: slept", time.Since(t0) >= 90*time.Millisecond)
	if len(os.Args) > 1 {
		r, err := http.Get(os.Args[1])
		if err != nil {
			fmt.Println("gt: http error", err)
			return
		}
		b, _ := io.ReadAll(r.Body)
		fmt.Printf("gt: http %d %s", r.StatusCode, b)
	}
}
