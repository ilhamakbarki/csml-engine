pipeline {
    agent any
    environment {
        DOCKER_IMAGE = "csml"
        REGISTRY_HOST = credentials("DOCKER_REGISTRY_HOST")
        APPROVAL = credentials("APPROVAL_RELEASE")
        NOTIF_API_KEY = credentials('NOTIF_API_KEY')
    }
    stages {
        stage('Build & Push Image Staging & Deploy on Staging') {
            when { branch 'staging_beta' }
            steps {
                script {
                  def tagLatest = "${REGISTRY_HOST}/${DOCKER_IMAGE}:staging_beta-latest"
                  def tagBuildNumber = "${REGISTRY_HOST}/${DOCKER_IMAGE}:staging_beta-${BUILD_NUMBER}"

                  echo 'Start Build Image Staging'
                  sh "docker build -t ${tagLatest} -f docker/sb.Dockerfile ."

                  echo 'Start Pushing Image'
                  docker.withRegistry("https://${REGISTRY_HOST}", "DOCKER_REGISTRY_USER") {
                      sh "docker push ${tagLatest}"
                      sh "docker tag ${tagLatest} ${tagBuildNumber}"
                      sh "docker push ${tagBuildNumber}"
                  }

                  echo 'Start Deploy on Staging'
                  sh "kubectl set image deployment csml csml=${tagBuildNumber} -n=csml-staging"
                }
            }
        }
        stage('Publish Approval') {
            when { tag "release-*" }
            steps {
                script{
                    sendNotification("Waiting Approval to Deploy on Production")
                    def tagName = env.TAG_NAME
                    def approvers = APPROVAL.split(',')
                    def userName = input message: "Do you want to deploy ${tagName}?", submitter: APPROVAL, submitterParameter: "userName"

                    if (!approvers.contains(userName)) {
                        error('This user is not approved to deploy to PROD.')
                    } else {
                        echo "Accepted by ${userName}"
                    }
                }
            }
        }
        stage('Build & Push Image Production & Deploy on Production') {
            when { tag "release-*" }
            steps {
                script {
                  def tagLatest = "${REGISTRY_HOST}/${DOCKER_IMAGE}:release-latest"
                  def tagBuildNumber = "${REGISTRY_HOST}/${DOCKER_IMAGE}:${TAG_NAME}-${BUILD_NUMBER}"

                  echo 'Start Build Image Staging'
                  sh "docker build -t ${tagLatest} -f docker/sb.Dockerfile ."

                  echo 'Start Pushing Image'
                  docker.withRegistry("https://${REGISTRY_HOST}", "DOCKER_REGISTRY_USER") {
                      sh "docker push ${tagLatest}"
                      sh "docker tag ${tagLatest} ${tagBuildNumber}"
                      sh "docker push ${tagBuildNumber}"
                  }

                  echo 'Start Deploy on Production'
                  sh "kubectl set image deployment csml csml=${tagBuildNumber} -n=csml-production"
                }
            }
        }
    }

    post {
        success {
            script {
                sendNotification("Success to deploy")
            }
        }
        failure {
            script {
                sendNotification("Failed to deploy")
            }
        }
    }
}

def sendNotification(message) {
    echo 'Sending Notification...'
    def tag = env.TAG_NAME ?: ''
    def branch = env.BRANCH_NAME ?: ''
    def NAME = env.TAG_NAME ?: env.BRANCH_NAME
    def cleanJobPath = env.JOB_NAME.replaceFirst('^/job', '').replaceAll('/$', '')
    def formattedJobPath = cleanJobPath.split('/').collect { "job/${it}" }.join('/')
    def link = "${env.PUBLIC_JENKINS_URL}${formattedJobPath}/${env.BUILD_NUMBER}/console"
    sh """
        curl --location 'https://webhooks.socialbot.dev/webhook/jenkins-deploy' \\
            --header 'Content-Type: application/json' \\
            --header 'x-api-key: ${NOTIF_API_KEY}' \\
            --data '{
                "message": "${message} Link : ${link}",
                "service": "${DOCKER_IMAGE}",
                "branch": "${branch}",
                "tag": "${tag}"
            }'
    """
}

def generateDockerBuildArgs(envContent) {
    def buildArgs = []
    def lines = envContent.readLines()
    lines.each { line ->
        def trimmedLine = line.trim()

        if (trimmedLine && !trimmedLine.startsWith('#')) {
            def parts = trimmedLine.split('=', 2)
            if (parts.size() == 2) {
                def key = parts[0].trim()
                def value = parts[1].trim()
                if (value.startsWith('"') && value.endsWith('"')) {
                    value = value.substring(1, value.length() - 1)
                }
                buildArgs << "--build-arg ${key}=\"${value}\""
            } else {
                println "Warning: Skip invalid line format in .env: ${line}"
            }
        }
    }
    return buildArgs.join(' ')
}
