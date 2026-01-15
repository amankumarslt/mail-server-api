import { useState, useEffect } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { ApiPlayground } from '@/components/ApiPlayground'
import './App.css'

// Dynamically point to Backend on same host, port 8080
const API_URL = `http://${window.location.hostname}:8080`

interface User {
  id: string
  email: string
  first_name?: string
  last_name?: string
  temp_alias?: string
}

interface Email {
  sender: string
  subject: string
  preview: string
  received_at: string
}

function App() {
  const [user, setUser] = useState<User | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [emails, setEmails] = useState<Email[]>([])
  const [loading, setLoading] = useState(false)
  const [manualToken, setManualToken] = useState('')

  // Check for auth callback on mount
  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const urlToken = params.get('token')
    const urlUser = params.get('user')

    if (urlToken) {
      handleTokenLogin(urlToken, urlUser)
    } else {
      const storedToken = localStorage.getItem('token')
      const storedUser = localStorage.getItem('user')
      if (storedToken) setToken(storedToken)
      if (storedUser) setUser(JSON.parse(storedUser))
    }
  }, [])

  const handleTokenLogin = (newToken: string, userStr?: string | null) => {
    setToken(newToken)
    localStorage.setItem('token', newToken)

    if (userStr) {
      try {
        const userData = JSON.parse(decodeURIComponent(userStr))
        setUser(userData)
        localStorage.setItem('user', JSON.stringify(userData))
      } catch (e) {
        console.error('Failed to parse user data')
      }
    } else {
      try {
        const payload = JSON.parse(atob(newToken.split('.')[1]))
        if (payload.sub) {
          const userData = { id: payload.sub, email: 'User' }
          setUser(userData)
          localStorage.setItem('user', JSON.stringify(userData))
        }
      } catch (e) {
        console.error('Failed to decode token')
      }
    }
    window.history.replaceState({}, document.title, '/')
  }

  const handleManualLogin = () => {
    if (!manualToken) return
    handleTokenLogin(manualToken, null)
  }

  const handleLogin = () => {
    const redirectUrl = encodeURIComponent(window.location.origin)
    window.location.href = `${API_URL}/auth/sso?redirect_to=${redirectUrl}`
  }

  const handleConnectGmail = () => {
    if (user) {
      const redirectUrl = encodeURIComponent(window.location.origin)
      window.location.href = `${API_URL}/connect/gmail?user_id=${user.id}&redirect_to=${redirectUrl}`
    }
  }

  const handleCreateTempAlias = async () => {
    if (!user || !token) return
    try {
      setLoading(true)
      const res = await fetch(`${API_URL}/temp-mail`, {
        method: 'POST',
        headers: { 'Authorization': `Bearer ${token}` }
      })
      const data = await res.json()
      if (data.alias) {
        const updatedUser = { ...user, temp_alias: data.alias }
        setUser(updatedUser)
        localStorage.setItem('user', JSON.stringify(updatedUser))
      }
    } catch (e) {
      console.error('Failed to create alias', e)
    } finally {
      setLoading(false)
    }
  }

  const handleDeleteTempAlias = async () => {
    if (!user || !token) return
    if (!confirm('Delete this alias? You will stop receiving mail for it.')) return
    try {
      await fetch(`${API_URL}/temp-mail/${user.id}`, {
        method: 'DELETE',
        headers: { 'Authorization': `Bearer ${token}` }
      })
      const updatedUser = { ...user, temp_alias: undefined }
      setUser(updatedUser)
      // We manually clear it from UI, waiting for re-login is optional but better UX directly
      delete updatedUser.temp_alias
      localStorage.setItem('user', JSON.stringify(updatedUser))
    } catch (e) {
      console.error('Failed to delete', e)
    }
  }

  const handleSyncEmails = async () => {
    if (!user || !token) return
    setLoading(true)
    try {
      // Trigger external sync (optional, fails silently if not connected)
      await fetch(`${API_URL}/sync/${user.id}`, {
        headers: { 'Authorization': `Bearer ${token}` }
      }).catch(() => { })

      // Fetch local DB emails (includes alias emails)
      await handleFetchEmails()
    } catch (e) {
      console.error('Sync failed:', e)
    }
    setLoading(false)
  }

  const handleFetchEmails = async () => {
    if (!user || !token) return
    try {
      // Use get_all_emails route for everyone
      const res = await fetch(`${API_URL}/emails/${user.id}`, {
        headers: { 'Authorization': `Bearer ${token}` }
      })
      const data = await res.json()

      if (Array.isArray(data)) {
        setEmails(data)
      } else if (data.sender) {
        setEmails([data])
      }
    } catch (e) {
      console.error('Fetch failed:', e)
    }
  }

  const handleLogout = () => {
    setUser(null)
    setToken(null)
    setEmails([])
    localStorage.removeItem('token')
    localStorage.removeItem('user')
  }

  return (
    <div className="min-h-screen bg-background p-8">
      <div className="max-w-4xl mx-auto space-y-8">
        {/* Header */}
        <div className="flex items-center justify-between">
          <h1 className="text-3xl font-bold text-foreground">📧 Mail Server</h1>
          {user && (
            <Button variant="outline" onClick={handleLogout}>
              Logout
            </Button>
          )}
        </div>

        {/* Login Card */}
        {!user ? (
          <div className="space-y-6 max-w-md mx-auto">
            <Card>
              <CardHeader>
                <CardTitle>Welcome</CardTitle>
                <CardDescription>Sign in to access your emails</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <Button onClick={handleLogin} className="w-full">
                  Sign in with WorkOS
                </Button>

                <div className="relative">
                  <div className="absolute inset-0 flex items-center">
                    <span className="w-full border-t" />
                  </div>
                  <div className="relative flex justify-center text-xs uppercase">
                    <span className="bg-background px-2 text-muted-foreground">
                      Or paste token
                    </span>
                  </div>
                </div>

                <div className="flex gap-2">
                  <Input
                    placeholder="eyJ..."
                    value={manualToken}
                    onChange={(e) => setManualToken(e.target.value)}
                  />
                  <Button onClick={handleManualLogin} variant="secondary">
                    Login
                  </Button>
                </div>
              </CardContent>
            </Card>
          </div>
        ) : (
          <>
            {/* User Info */}
            <Card>
              <CardHeader>
                <CardTitle className="flex justify-between items-center">
                  <span>Hello, {user.first_name || 'User'}!</span>
                </CardTitle>
                <CardDescription>
                  ID: {user.id} <br />
                  Email: {user.email}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="flex gap-4 flex-wrap">
                  <Button onClick={handleConnectGmail} variant="secondary">
                    Connect Gmail
                  </Button>
                  <Button onClick={handleSyncEmails} disabled={loading}>
                    {loading ? 'Syncing...' : 'Sync & Refresh'}
                  </Button>
                </div>

                <div className="border-t pt-4">
                  <h3 className="font-semibold mb-2">Disposable Alias</h3>
                  {user.temp_alias ? (
                    <div className="flex items-center gap-4 bg-muted p-3 rounded-md">
                      <span className="font-mono text-sm">{user.temp_alias}@localhost</span>
                      <div className="ml-auto flex gap-2">
                        <Button size="sm" variant="outline" onClick={() => navigator.clipboard.writeText(`${user.temp_alias}@localhost`)}>
                          Copy
                        </Button>
                        <Button size="sm" variant="destructive" onClick={handleDeleteTempAlias}>
                          Delete
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center gap-4">
                      <Button onClick={handleCreateTempAlias} variant="outline" disabled={loading}>
                        Generate Disposable Alias
                      </Button>
                      <span className="text-sm text-muted-foreground">Receive mail temporarily without sharing your real address.</span>
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* Emails List */}
            {emails.length > 0 ? (
              <div className="space-y-4">
                <h2 className="text-xl font-semibold">Inbox ({emails.length})</h2>
                {emails.map((email, i) => (
                  <Card key={i}>
                    <CardHeader>
                      <CardTitle className="text-lg">{email.subject || '(No Subject)'}</CardTitle>
                      <CardDescription>From: {email.sender}</CardDescription>
                    </CardHeader>
                    <CardContent>
                      <p className="text-muted-foreground">{email.preview}</p>
                      <p className="text-xs text-muted-foreground mt-2">
                        {new Date(email.received_at).toLocaleString()}
                      </p>
                    </CardContent>
                  </Card>
                ))}
              </div>
            ) : (
              <div className="text-center text-muted-foreground py-10">
                No emails found. Click Sync to fetch.
              </div>
            )}

            {/* API Playground */}
            {token && (
              <div className="border-t pt-8">
                <ApiPlayground token={token} userId={user.id} />
              </div>
            )}
          </>
        )}
      </div>
    </div>
  )
}

export default App
