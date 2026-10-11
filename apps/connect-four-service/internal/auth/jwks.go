package auth

import (
	"context"
	"fmt"
	"log"
	"strings"
	"sync"

	"github.com/lestrrat-go/jwx/v2/jwk"
	"github.com/lestrrat-go/jwx/v2/jwt"
)

type User struct {
	ID                   string
	ActiveOrganizationID *string
	ActiveTeamID         *string
	ActiveOrgRole        *string
}

var (
	jwksCache *jwk.Cache
	jwksURLs  []string
	once      sync.Once
)

// Init registers one or more JWKS endpoints (the Go auth-service used by
// web-beta). A token is accepted if it verifies against any registered key set.
// Empty URLs are ignored, and a source that fails to warm at startup is logged
// but non-fatal so the server still boots when the auth backend is temporarily
// down.
func Init(authServerURLs ...string) {
	once.Do(func() {
		jwksCache = jwk.NewCache(context.Background())
		for _, base := range authServerURLs {
			if base == "" {
				continue
			}
			url := base + "/api/auth/jwks"
			if err := jwksCache.Register(url); err != nil {
				log.Printf("auth: failed to register JWKS URL %s: %v", url, err)
				continue
			}
			if _, err := jwksCache.Refresh(context.Background(), url); err != nil {
				log.Printf("auth: failed to warm JWKS %s (will retry lazily): %v", url, err)
			}
			jwksURLs = append(jwksURLs, url)
		}
		if len(jwksURLs) == 0 {
			panic("auth: no JWKS URLs registered")
		}
	})
}

func VerifyToken(tokenStr string) (*User, error) {
	var lastErr error
	for _, url := range jwksURLs {
		keySet, err := jwksCache.Get(context.Background(), url)
		if err != nil {
			lastErr = fmt.Errorf("fetch jwks %s: %w", url, err)
			continue
		}
		parsed, err := jwt.Parse([]byte(tokenStr), jwt.WithKeySet(keySet), jwt.WithValidate(true))
		if err != nil {
			lastErr = fmt.Errorf("verify token: %w", err)
			continue
		}
		return userFromToken(parsed)
	}
	if lastErr == nil {
		lastErr = fmt.Errorf("no JWKS sources available")
	}
	return nil, lastErr
}

func userFromToken(tok jwt.Token) (*User, error) {
	claims := tok.PrivateClaims()

	id := stringClaim(claims, "id")
	if id == "" {
		id = tok.Subject()
	}
	if id == "" {
		return nil, fmt.Errorf("missing id claim")
	}

	return &User{
		ID:                   id,
		ActiveOrganizationID: optStringClaim(claims, "activeOrganizationId"),
		ActiveTeamID:         optStringClaim(claims, "activeTeamId"),
		ActiveOrgRole:        optStringClaim(claims, "activeOrgRole"),
	}, nil
}

func ParseBearer(authHeader string) (string, bool) {
	if !strings.HasPrefix(authHeader, "Bearer ") {
		return "", false
	}
	return strings.TrimPrefix(authHeader, "Bearer "), true
}

func stringClaim(claims map[string]any, key string) string {
	v, _ := claims[key].(string)
	return v
}

func optStringClaim(claims map[string]any, key string) *string {
	v, ok := claims[key].(string)
	if !ok || v == "" {
		return nil
	}
	return &v
}
