import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'


interface ApiPlaygroundProps {
    token: string
    userId: string
}

// Production API URL
const API_URL = 'https://api.rapidxoxo.dpdns.org';

export function ApiPlayground({ token, userId }: ApiPlaygroundProps) {
    const [response, setResponse] = useState<string | null>(null)
    const [activeRequest, setActiveRequest] = useState<string | null>(null)
    const [isLoading, setIsLoading] = useState(false)

    const requests = [
        {
            name: 'Sync Emails',
            method: 'GET',
            endpoint: `/sync/${userId}`,
            description: 'Fetch latest emails from Gmail/IMAP provider',
            curl: `curl -H "Authorization: Bearer ${token}" ${API_URL}/sync/${userId}`
        },
        {
            name: 'Get Latest Email',
            method: 'GET',
            endpoint: `/latest/${userId}`,
            description: 'Get the most recent email stored in the database',
            curl: `curl -H "Authorization: Bearer ${token}" ${API_URL}/latest/${userId}`
        },
        {
            name: 'Get All Emails',
            method: 'GET',
            endpoint: `/emails/${userId}`,
            description: 'Get all emails for this user (including temp mail)',
            curl: `curl -H "Authorization: Bearer ${token}" ${API_URL}/emails/${userId}`
        }
    ]

    const handleCopy = (text: string) => {
        navigator.clipboard.writeText(text)
    }

    const handleRun = async (req: typeof requests[0]) => {
        setIsLoading(true)
        setActiveRequest(req.name)
        setResponse(null)
        try {
            const res = await fetch(`${API_URL}${req.endpoint}`, {
                method: req.method,
                headers: {
                    'Authorization': `Bearer ${token}`
                }
            })
            const data = await res.json()
            setResponse(JSON.stringify(data, null, 2))
        } catch (e) {
            setResponse(`Error: ${e}`)
        }
        setIsLoading(false)
    }

    return (
        <div className="space-y-6">
            <h2 className="text-2xl font-bold">🛠️ API Playground</h2>
            <div className="grid gap-6">
                {requests.map((req) => (
                    <Card key={req.name} className="overflow-hidden">
                        <CardHeader className="bg-muted/50">
                            <div className="flex items-center justify-between">
                                <div>
                                    <CardTitle className="flex items-center gap-2">
                                        <span className="px-2 py-0.5 rounded text-xs font-mono bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300">
                                            {req.method}
                                        </span>
                                        {req.name}
                                    </CardTitle>
                                    <CardDescription className="mt-1.5">{req.description}</CardDescription>
                                </div>
                                <div className="flex gap-2">
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        onClick={() => handleCopy(req.curl)}
                                    >
                                        Copy cURL
                                    </Button>
                                    <Button
                                        size="sm"
                                        onClick={() => handleRun(req)}
                                        disabled={isLoading}
                                    >
                                        {isLoading && activeRequest === req.name ? 'Running...' : 'Run Request'}
                                    </Button>
                                </div>
                            </div>
                        </CardHeader>
                        <CardContent className="p-4 space-y-4">
                            <div className="bg-zinc-950 rounded-md p-4 overflow-x-auto">
                                <code className="text-sm font-mono text-zinc-50 whitespace-pre-wrap break-all">
                                    {req.curl}
                                </code>
                            </div>

                            {activeRequest === req.name && response && (
                                <div className="mt-4 animate-in fade-in slide-in-from-top-2">
                                    <h4 className="text-sm font-medium mb-2">Response:</h4>
                                    <div className="bg-zinc-950 rounded-md p-4 overflow-x-auto max-h-[300px]">
                                        <pre className="text-xs font-mono text-green-400">
                                            {response}
                                        </pre>
                                    </div>
                                </div>
                            )}
                        </CardContent>
                    </Card>
                ))}
            </div>
        </div>
    )
}
